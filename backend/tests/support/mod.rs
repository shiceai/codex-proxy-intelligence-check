//! 通过公开 RPC 会话驱动插件，模拟宿主回调与调度授权（改自官方 workbench 示例）

use std::{collections::BTreeMap, time::Duration};

use codex_proxy_intelligence_check::{PLUGIN_ID, call_budgets, manifest, plugin};
use gateway_plugin_sdk::{
    CallContext, ErrorCode, Frame, Handshake, Message, PROTOCOL_VERSION, PluginFault,
    ResolvedCallBudgets, Stage,
    call::{
        host::ModelEventBatch,
        model::{ExecutionEvent, WireEvent, WirePayload},
    },
    client::{PluginSession, SessionConfig, read_frame, write_frame},
};
use serde_json::{Value, json};
use tokio::{
    io::{DuplexStream, ReadHalf, WriteHalf},
    task::JoinHandle,
};

const PIPE_BUFFER_BYTES: usize = 1024 * 1024;
pub type CallbackReply = Result<(Value, Vec<u8>), PluginFault>;

pub struct Peer {
    reader: ReadHalf<DuplexStream>,
    writer: WriteHalf<DuplexStream>,
    task: JoinHandle<()>,
    next_id: u64,
    pub budgets: ResolvedCallBudgets,
    suspended: BTreeMap<u64, u64>,
    callbacks: BTreeMap<u64, u64>,
}

impl Peer {
    pub async fn start() -> Self {
        let (host, transport) = tokio::io::duplex(PIPE_BUFFER_BYTES * 2);
        let (reader, writer) = tokio::io::split(transport);
        let task = tokio::spawn(async move {
            let mut session = PluginSession::accept(reader, writer, SessionConfig::default())
                .await
                .unwrap();
            session.set_call_budgets(call_budgets()).unwrap();
            session.run(plugin().unwrap()).await.unwrap();
        });
        let (reader, writer) = tokio::io::split(host);
        let mut peer = Self {
            reader,
            writer,
            task,
            next_id: 0,
            budgets: ResolvedCallBudgets::resolve(&Default::default()).unwrap(),
            suspended: BTreeMap::new(),
            callbacks: BTreeMap::new(),
        };
        let manifest = manifest().unwrap();
        peer.send(Frame::control(Message::Hello {
            handshake: Handshake {
                protocol_version: PROTOCOL_VERSION,
                artifact_sha256: "a".repeat(64),
                plugin_id: PLUGIN_ID.to_owned(),
                instance_id: "test-instance".to_owned(),
                generation: 1,
                incarnation: "test-session".to_owned(),
                configuration: json!({}),
                contributes: manifest.contributes,
            },
        }))
        .await;
        let Message::Ready { call_budgets, .. } = peer.receive().await.message else {
            panic!("预期插件握手完成");
        };
        peer.budgets = ResolvedCallBudgets::resolve(&call_budgets).unwrap();
        peer
    }

    pub async fn call_with(
        &mut self,
        method: &str,
        stage: Stage,
        params: Value,
        payload: Vec<u8>,
        mut callback: impl FnMut(&str, &Value, &[u8]) -> CallbackReply,
    ) -> Frame {
        self.next_id += 1;
        let id = self.next_id * 2 - 1;
        self.send(Frame {
            message: Message::Call {
                id,
                method: method.to_owned(),
                context: CallContext {
                    request: None,
                    call_id: id,
                    instance_id: "test-instance".to_owned(),
                    generation: 1,
                    incarnation: "test-session".to_owned(),
                    stage,
                    timeout_ms: u64::try_from(self.budgets.budget(stage).as_millis()).unwrap(),
                    resource_stream: false,
                    resource_scope_id: "test-scope".to_owned(),
                    request_id: Some("test-request".to_owned()),
                    attempt_id: None,
                    account_id: None,
                    credential_revision: None,
                },
                params,
            },
            payload,
        })
        .await;
        loop {
            let frame = self.receive().await;
            match frame.message {
                Message::Callback {
                    id: callback_id,
                    parent_id,
                    method,
                    params,
                } => {
                    assert_eq!(parent_id, id);
                    let response = match callback(&method, &params, &frame.payload) {
                        Ok((result, payload)) => Frame {
                            message: Message::Result {
                                id: callback_id,
                                result,
                            },
                            payload,
                        },
                        Err(error) => Frame::control(Message::Error {
                            id: callback_id,
                            error,
                        }),
                    };
                    self.send(response).await;
                }
                Message::Result { id: reply_id, .. } | Message::Error { id: reply_id, .. } => {
                    assert_eq!(reply_id, id);
                    assert!(self.callbacks.is_empty());
                    assert!(!self.suspended.contains_key(&id));
                    return frame;
                }
                _ => panic!("出现未预期的插件消息：{frame:?}"),
            }
        }
    }

    pub async fn api_with_query(
        &mut self,
        method: &str,
        path: &str,
        query: &str,
        data: Option<Value>,
        callback: impl FnMut(&str, &Value, &[u8]) -> CallbackReply,
    ) -> (u64, Value) {
        let params = json!({
            "method": method, "path": path, "query": query,
            "content_type": data.as_ref().map(|_| "application/json"),
        });
        let payload = data.map_or_else(Vec::new, |value| serde_json::to_vec(&value).unwrap());
        let frame = self
            .call_with(
                "management.handle",
                Stage::Management,
                params,
                payload,
                callback,
            )
            .await;
        let result = result(&frame);
        (
            result["status"].as_u64().unwrap(),
            serde_json::from_slice(&frame.payload).unwrap(),
        )
    }

    pub async fn api(
        &mut self,
        method: &str,
        path: &str,
        data: Option<Value>,
        callback: impl FnMut(&str, &Value, &[u8]) -> CallbackReply,
    ) -> (u64, Value) {
        self.api_with_query(method, path, "", data, callback).await
    }

    async fn send(&mut self, frame: Frame) {
        write_frame(&mut self.writer, &frame).await.unwrap();
    }

    async fn receive(&mut self) -> Frame {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(10), read_frame(&mut self.reader))
                .await
                .expect("插件响应超时")
                .unwrap();
            match frame.message {
                Message::Suspend { id, epoch } => {
                    assert!(epoch > 0 && self.suspended.insert(id, epoch).is_none());
                }
                Message::Resume { id, epoch } => {
                    assert_eq!(self.suspended.remove(&id), Some(epoch));
                    self.send(Frame::control(Message::Resumed { id, epoch }))
                        .await;
                }
                Message::CallbackConsumed { id, parent_id } => {
                    assert_eq!(self.callbacks.remove(&id), Some(parent_id));
                }
                Message::Callback { id, parent_id, .. } => {
                    assert!(self.callbacks.insert(id, parent_id).is_none());
                    return frame;
                }
                _ => return frame,
            }
        }
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn result(frame: &Frame) -> &Value {
    let Message::Result { result, .. } = &frame.message else {
        panic!("预期成功响应，实际收到：{frame:?}");
    };
    result
}

pub fn no_callback(method: &str, _: &Value, _: &[u8]) -> CallbackReply {
    panic!("出现未预期的宿主回调：{method}");
}

/// 一条完整、合法的检测结果记录。
pub fn sample_result(id: &str) -> Value {
    json!({
        "id": id, "created_at_ms": 1_700_000_000_000_u64,
        "account_id": "acct_1", "account_label": "a@x.com",
        "key_id": "key_1", "key_name": "测试 Key",
        "model": "gpt-5.5", "response_model": "gpt-5.5", "effort": "high",
        "question_id": "candy", "question_name": "糖果配对", "expected": "21", "match": "exact",
        "verdict": "normal", "answer_excerpt": "21", "error": null,
        "duration_ms": 1200, "input_tokens": 10, "output_tokens": 20, "reasoning_tokens": 15,
        "request_id": "req_1",
    })
}

/// 一个完整的 `AuthRuntimeAccount` JSON。
pub fn account(id: &str, provider: &str, enabled: bool, email: Option<&str>, name: &str) -> Value {
    json!({
        "account_id": id, "provider_id": provider, "credential_revision": 1,
        "name": name, "email": email,
        "upstream_user_id": "user_1", "upstream_account_id": null,
        "plan_type": "plus", "authentication_kind": "oauth",
        "enabled": enabled, "credential_state": "active", "has_refresh_token": true,
        "access_token_expires_at_ms": null, "next_refresh_at_ms": null,
    })
}

/// 以 SSE JSON 载荷编码的模型事件批次。
pub fn batch(events: Vec<Value>) -> Vec<u8> {
    encode_events(events.into_iter().map(|data| WirePayload::Json {
        event: None,
        data,
        id: None,
        retry: None,
        raw_sse: None,
    }))
}

/// 以 WebSocket 原文编码的模型事件批次（无解析视图）。
pub fn ws_batch(events: Vec<Value>) -> Vec<u8> {
    encode_events(events.into_iter().map(|data| WirePayload::WebSocket {
        event: None,
        data: None,
        message: serde_json::to_vec(&data).unwrap(),
    }))
}

fn encode_events(payloads: impl Iterator<Item = WirePayload>) -> Vec<u8> {
    ModelEventBatch {
        events: payloads
            .map(|payload| ExecutionEvent {
                facts: Vec::new(),
                wire: Some(WireEvent {
                    protocol: "openai".to_owned(),
                    payload,
                }),
                host: None,
            })
            .collect(),
    }
    .encode()
    .unwrap()
}

/// 内存版私有状态：按版本号做 CAS，可注入一次"其他写入者抢先写入"。
#[derive(Default)]
pub struct StateStore {
    records: BTreeMap<(String, String), (u64, Value)>,
    concurrent_write: Option<(String, String, Value)>,
    pub failing_puts: Vec<(String, ErrorCode)>,
    /// 让某个命名空间的第 N 次写入（从 1 计）以 Fault 失败。
    pub fail_nth_put: Option<(String, usize)>,
    pub puts: Vec<(String, String)>,
}

impl StateStore {
    pub fn with(mut self, namespace: &str, key: &str, value: Value) -> Self {
        self.records
            .insert((namespace.to_owned(), key.to_owned()), (1, value));
        self
    }

    pub fn get(&self, namespace: &str, key: &str) -> Option<&Value> {
        self.records
            .get(&(namespace.to_owned(), key.to_owned()))
            .map(|(_, value)| value)
    }

    /// 下一次写入该记录前，先模拟另一个写入者把它改成 `value`，使本次写入版本冲突。
    pub fn race_next_put(&mut self, namespace: &str, key: &str, value: Value) {
        self.concurrent_write = Some((namespace.to_owned(), key.to_owned(), value));
    }

    pub fn handle(&mut self, method: &str, params: &Value) -> Option<CallbackReply> {
        let id = (
            params["namespace"].as_str()?.to_owned(),
            params["key"].as_str()?.to_owned(),
        );
        match method {
            "host.state.get" => {
                let record = self.records.get(&id).map(
                    |(version, value)| json!({"schema_version":1,"version":version,"value":value}),
                );
                Some(Ok((json!({ "record": record }), Vec::new())))
            }
            "host.state.put" => {
                self.puts.push(id.clone());
                let nth = self.puts.iter().filter(|(ns, _)| *ns == id.0).count();
                if self.fail_nth_put.as_ref() == Some(&(id.0.clone(), nth)) {
                    return Some(Err(PluginFault::new(
                        ErrorCode::Fault,
                        "状态服务暂时不可用",
                    )));
                }
                if let Some(position) = self.failing_puts.iter().position(|(ns, _)| *ns == id.0) {
                    let (_, code) = self.failing_puts.remove(position);
                    return Some(Err(PluginFault::new(code, "写入失败")));
                }
                if let Some((namespace, key, value)) = self.concurrent_write.take() {
                    if (namespace.as_str(), key.as_str()) == (id.0.as_str(), id.1.as_str()) {
                        let version = self.records.get(&id).map_or(0, |(version, _)| *version) + 1;
                        self.records.insert(id, (version, value));
                        return Some(Err(PluginFault::new(
                            ErrorCode::Conflict,
                            "version mismatch",
                        )));
                    }
                    self.concurrent_write = Some((namespace, key, value));
                }
                let current = self.records.get(&id).map(|(version, _)| *version);
                if params["expected_version"].as_u64() != current {
                    return Some(Err(PluginFault::new(
                        ErrorCode::Conflict,
                        "version mismatch",
                    )));
                }
                let version = current.unwrap_or(0) + 1;
                self.records.insert(id, (version, params["value"].clone()));
                Some(Ok((json!({ "version": version }), Vec::new())))
            }
            _ => None,
        }
    }
}

/// 可配置的模拟宿主：状态、Key、账号与按账号区分的模型事件流。
pub struct Host {
    pub state: StateStore,
    pub keys: Vec<Value>,
    pub accounts: BTreeMap<String, Value>,
    /// 账号 → 该账号模型流的（事件数，编码后的批次）。
    pub streams: BTreeMap<String, (u32, Vec<u8>)>,
    pub execute_error: Option<&'static str>,
    pub calls: Vec<String>,
    pub executed: Vec<Value>,
}

impl Host {
    pub fn new(state: StateStore) -> Self {
        Self {
            state,
            keys: vec![json!({"id":"key_1","name":"主 Key","enabled":true})],
            accounts: BTreeMap::new(),
            streams: BTreeMap::new(),
            execute_error: None,
            calls: Vec::new(),
            executed: Vec::new(),
        }
    }

    pub fn count(&self, method: &str) -> usize {
        self.calls
            .iter()
            .filter(|call| call.as_str() == method)
            .count()
    }

    pub fn callback(&mut self, method: &str, params: &Value, payload: &[u8]) -> CallbackReply {
        self.calls.push(method.to_owned());
        if let Some(reply) = self.state.handle(method, params) {
            return reply;
        }
        match method {
            "host.keys.list" => Ok((
                json!({ "keys": self.keys, "next_cursor": null }),
                Vec::new(),
            )),
            "host.auth.get_runtime" => {
                let request: Value = serde_json::from_slice(payload).unwrap();
                let id = request["account_id"].as_str().unwrap();
                match self.accounts.get(id) {
                    Some(account) => Ok((json!({}), serde_json::to_vec(account).unwrap())),
                    None => Err(PluginFault::new(
                        ErrorCode::InvalidInput,
                        "account not found",
                    )),
                }
            }
            "host.model.execute_stream" => {
                self.executed.push(params.clone());
                if let Some(message) = self.execute_error {
                    return Err(PluginFault::new(ErrorCode::InvalidInput, message));
                }
                let account = params["account_id"].as_str().unwrap();
                Ok((
                    json!({ "request_id": format!("req-{account}"), "stream": format!("s-{account}") }),
                    Vec::new(),
                ))
            }
            "host.model.stream_read" => {
                let stream = params["stream"].as_str().unwrap();
                let (events, payload) = self.streams[stream.trim_start_matches("s-")].clone();
                Ok((json!({ "events": events, "end": true }), payload))
            }
            "host.model.stream_close" => Ok((json!({}), Vec::new())),
            _ => panic!("出现未预期的宿主回调：{method}"),
        }
    }
}

impl Peer {
    /// 以维护阶段调用一次后台处理，返回宿主收到的结果帧。
    pub async fn reconcile(
        &mut self,
        callback: impl FnMut(&str, &Value, &[u8]) -> CallbackReply,
    ) -> Frame {
        self.call_with(
            "plugin.reconcile",
            Stage::Maintenance,
            json!({}),
            Vec::new(),
            callback,
        )
        .await
    }
}

//! 宿主回调的类型化封装；插件只通过这些回调读写宿主数据。

use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::host::{
        AuthGetRequest, AuthListRequest, AuthListResult, AuthRuntimeAccount, ClientKey,
        KeyListRequest, ModelEventBatch, ModelExecuteRequest, ModelListRequest, ModelListResult,
        ModelStreamCloseRequest, ModelStreamReadRequest, ModelStreamReadResult, ModelStreamResult,
        StateGetRequest, StateGetResult, StatePutRequest, StatePutResult, StateRecord,
    },
    client::{HostClient, SessionError},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeSet;

use crate::responses::event_json;

const PAGE_SIZE: u16 = 100;
const MAXIMUM_PAGES: usize = 100;
const PROVIDER: &str = "openai";
/// 单次 `stream_read` 最多交付的事件字节。
const STREAM_READ_BYTES: u32 = 256 * 1024;

/// 分页读取已启用的 OpenAI 账号；只含非秘密的运行投影。
pub(crate) async fn openai_accounts(
    host: &HostClient,
) -> Result<Vec<AuthRuntimeAccount>, PluginFault> {
    let mut accounts = Vec::new();
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    loop {
        let page: AuthListResult = payload(
            host,
            "host.auth.list",
            &AuthListRequest {
                provider_id: Some(PROVIDER.to_owned()),
                cursor,
                limit: PAGE_SIZE,
            },
        )
        .await?;
        accounts.extend(
            page.accounts
                .into_iter()
                .filter(|account| account.enabled && account.provider_id == PROVIDER),
        );
        cursor = next_page(page.next_cursor, &mut seen, "账号分页异常")?;
        if cursor.is_none() {
            return Ok(accounts);
        }
    }
}

/// 账号的当前运行投影；不含原始凭据。
pub(crate) async fn runtime_account(
    host: &HostClient,
    account_id: &str,
) -> Result<AuthRuntimeAccount, PluginFault> {
    payload(
        host,
        "host.auth.get_runtime",
        &AuthGetRequest {
            account_id: account_id.to_owned(),
        },
    )
    .await
}

/// 经宿主的准入、租约、计量链路发起流式模型请求；正文放在二进制载荷。
pub(crate) async fn execute_stream(
    host: &HostClient,
    request: &ModelExecuteRequest,
    body: &Value,
) -> Result<ModelStreamResult, PluginFault> {
    let reply = host
        .call(
            "host.model.execute_stream",
            serde_json::to_value(request).map_err(|_| invalid_callback())?,
            serde_json::to_vec(body).map_err(|_| invalid_callback())?,
        )
        .await
        .map_err(SessionError::into_plugin_fault)?;
    serde_json::from_value(reply.result).map_err(|_| invalid_callback())
}

/// 读取一批事件，返回其中的 Responses JSON 事件、是否结束以及本批载荷字节数。
pub(crate) async fn stream_read(
    host: &HostClient,
    stream: &str,
) -> Result<(Vec<Value>, bool, usize), PluginFault> {
    let reply = host
        .call(
            "host.model.stream_read",
            serde_json::to_value(ModelStreamReadRequest {
                stream: stream.to_owned(),
                maximum_bytes: STREAM_READ_BYTES,
            })
            .map_err(|_| invalid_callback())?,
            Vec::new(),
        )
        .await
        .map_err(SessionError::into_plugin_fault)?;
    let state: ModelStreamReadResult =
        serde_json::from_value(reply.result).map_err(|_| invalid_callback())?;
    let events = if reply.payload.is_empty() {
        Vec::new()
    } else {
        ModelEventBatch::decode(&reply.payload)
            .map_err(|_| fault("模型事件批次格式错误"))?
            .events
    };
    if usize::try_from(state.events).ok() != Some(events.len()) {
        return Err(fault("模型事件数量不一致"));
    }
    let mut values = Vec::new();
    for event in events {
        let Some(wire) = event.wire else {
            continue;
        };
        if wire.protocol != PROVIDER {
            return Err(fault("模型返回了不匹配的协议"));
        }
        if let Some(value) = event_json(&wire.payload).map_err(|message| fault(&message))? {
            values.push(value);
        }
    }
    Ok((values, state.end, reply.payload.len()))
}

pub(crate) async fn stream_close(host: &HostClient, stream: &str) -> Result<(), PluginFault> {
    let _: Value = metadata(
        host,
        "host.model.stream_close",
        &ModelStreamCloseRequest {
            stream: stream.to_owned(),
        },
    )
    .await?;
    Ok(())
}

/// 分页读取已启用的 Client Key；宿主只返回 ID、名称与启用状态。
pub(crate) async fn enabled_keys(host: &HostClient) -> Result<Vec<ClientKey>, PluginFault> {
    let mut keys = Vec::new();
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    loop {
        let page = host
            .list_keys(KeyListRequest {
                cursor,
                limit: PAGE_SIZE,
            })
            .await?;
        keys.extend(page.keys.into_iter().filter(|key| key.enabled));
        cursor = next_page(page.next_cursor, &mut seen, "Key 分页异常")?;
        if cursor.is_none() {
            return Ok(keys);
        }
    }
}

/// 所选 Key 当前可见的 OpenAI 模型。
pub(crate) async fn models(host: &HostClient, key_id: &str) -> Result<Vec<String>, PluginFault> {
    let result: ModelListResult = metadata(
        host,
        "host.models.list",
        &ModelListRequest {
            client_key_id: key_id.to_owned(),
            protocol: PROVIDER.to_owned(),
            client_version: concat!("intelligence-check/", env!("CARGO_PKG_VERSION")).to_owned(),
        },
    )
    .await?;
    Ok(result.models)
}

/// 页面展示的账号名：优先去除首尾空白后的邮箱，没有邮箱时用账号名称。
pub(crate) fn account_label(account: &AuthRuntimeAccount) -> String {
    account
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .unwrap_or(&account.name)
        .to_owned()
}

fn next_page(
    next: Option<String>,
    seen: &mut BTreeSet<String>,
    message: &str,
) -> Result<Option<String>, PluginFault> {
    match next {
        None => Ok(None),
        Some(next) if seen.len() < MAXIMUM_PAGES && seen.insert(next.clone()) => Ok(Some(next)),
        Some(_) => Err(PluginFault::new(ErrorCode::Fault, message)),
    }
}

pub(crate) async fn get_state(
    host: &HostClient,
    namespace: &str,
    key: &str,
) -> Result<Option<StateRecord>, PluginFault> {
    let result: StateGetResult = metadata(
        host,
        "host.state.get",
        &StateGetRequest {
            namespace: namespace.to_owned(),
            key: key.to_owned(),
        },
    )
    .await?;
    Ok(result.record)
}

/// `expected_version = None` 表示仅当记录不存在时创建。
pub(crate) async fn put_state(
    host: &HostClient,
    namespace: &str,
    key: &str,
    value: Value,
    expected_version: Option<u64>,
) -> Result<u64, PluginFault> {
    let result: StatePutResult = metadata(
        host,
        "host.state.put",
        &StatePutRequest {
            namespace: namespace.to_owned(),
            key: key.to_owned(),
            value,
            expected_version,
        },
    )
    .await?;
    Ok(result.version)
}

/// 参数与结果都放在 JSON 控制段、没有二进制载荷的回调。
async fn metadata<I: Serialize, O: DeserializeOwned>(
    host: &HostClient,
    method: &str,
    input: &I,
) -> Result<O, PluginFault> {
    let reply = host
        .call(
            method,
            serde_json::to_value(input).map_err(|_| invalid_callback())?,
            Vec::new(),
        )
        .await
        .map_err(SessionError::into_plugin_fault)?;
    serde_json::from_value(reply.result).map_err(|_| invalid_callback())
}

/// 账号回调：控制参数为空对象，请求与结果都是二进制载荷中的 JSON。
async fn payload<I: Serialize, O: DeserializeOwned>(
    host: &HostClient,
    method: &str,
    input: &I,
) -> Result<O, PluginFault> {
    let reply = host
        .call(
            method,
            json!({}),
            serde_json::to_vec(input).map_err(|_| invalid_callback())?,
        )
        .await
        .map_err(SessionError::into_plugin_fault)?;
    serde_json::from_slice(&reply.payload).map_err(|_| invalid_callback())
}

fn invalid_callback() -> PluginFault {
    fault("宿主回调返回了无效响应")
}

fn fault(message: &str) -> PluginFault {
    PluginFault::new(ErrorCode::Fault, message)
}

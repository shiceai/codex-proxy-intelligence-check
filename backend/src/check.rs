//! 执行一次检测：显式指定账号与 Key 发起 Responses 请求，读完事件后判分。

use crate::{
    host_calls,
    judge::passes,
    now_ms,
    questions::Question,
    responses::{Completion, complete},
    results::{CheckResult, Effort, Verdict, excerpt, new_id},
};
use gateway_plugin_sdk::{
    call::host::{AuthRuntimeAccount, ClientKey, ModelExecuteRequest, ModelOperation},
    client::HostClient,
};
use serde_json::{Value, json};
use std::{future::Future, time::Duration};
use tokio::time::Instant;

/// 单题上限；宿主页面桥对一次管理请求最多等待 180 秒。
pub(crate) const CHECK_TIMEOUT: Duration = Duration::from_secs(150);
const MAXIMUM_STREAM_BYTES: usize = 1024 * 1024;
const PROVIDER: &str = "openai";

/// 一次检测的对象；调用方已核对账号、Key 与题目可用。
pub(crate) struct Target<'a> {
    pub account: &'a AuthRuntimeAccount,
    pub account_label: String,
    pub key: &'a ClientKey,
    pub model: &'a str,
    pub effort: Option<Effort>,
    pub question: &'a Question,
}

/// 检测请求只有题面这一条 user 消息：无 system，不带工具。
pub(crate) fn request_body(model: &str, prompt: &str, effort: Option<Effort>) -> Value {
    let mut body = json!({
        "model": model,
        "stream": true,
        "store": false,
        "instructions": "",
        "input": [{ "role": "user", "content": [{ "type": "input_text", "text": prompt }] }],
    });
    if let Some(effort) = effort {
        body["reasoning"] = json!({ "effort": effort });
    }
    body
}

/// 在整题共享的截止时间前完成；打开执行流与读取事件共用同一个截止时间。
pub(crate) async fn before<T>(
    deadline: Instant,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::time::timeout_at(deadline, future)
        .await
        .unwrap_or_else(|_| Err(format!("检测超过 {} 秒未完成", CHECK_TIMEOUT.as_secs())))
}

/// 循环读取事件批次直到流结束，受截止时间与总字节上限约束。
pub(crate) async fn collect_events<F, Fut>(
    deadline: Instant,
    mut next_batch: F,
) -> Result<Vec<Value>, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(Vec<Value>, bool, usize), String>>,
{
    let read = async {
        let mut events = Vec::new();
        let mut bytes = 0_usize;
        loop {
            let (batch, end, size) = next_batch().await?;
            bytes = bytes.saturating_add(size);
            if bytes > MAXIMUM_STREAM_BYTES {
                return Err("模型响应超过 1 MiB".to_owned());
            }
            events.extend(batch);
            if end {
                return Ok(events);
            }
        }
    };
    before(deadline, read).await
}

/// 执行并判分；宿主拒绝、上游失败与超时都记为"错误"，不影响通过率。
pub(crate) async fn run(host: &HostClient, target: Target<'_>) -> CheckResult {
    let created_at_ms = now_ms();
    let started = Instant::now();
    let (request_id, outcome) = execute(host, &target).await;
    let question = target.question;
    let mut result = CheckResult {
        id: new_id(created_at_ms),
        created_at_ms,
        account_id: target.account.account_id.clone(),
        account_label: target.account_label,
        key_id: target.key.id.clone(),
        key_name: target.key.name.clone(),
        model: target.model.to_owned(),
        response_model: None,
        effort: target.effort,
        question_id: question.id.clone(),
        question_name: question.name.clone(),
        expected: question.expected.clone(),
        match_mode: question.match_mode,
        verdict: Verdict::Error,
        answer_excerpt: String::new(),
        error: None,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        input_tokens: None,
        output_tokens: None,
        reasoning_tokens: None,
        request_id,
    };
    match outcome {
        Ok(completion) => {
            result.verdict = if passes(&completion.answer, &question.expected, question.match_mode)
            {
                Verdict::Normal
            } else {
                Verdict::Failed
            };
            result.answer_excerpt = excerpt(&completion.answer);
            result.response_model = completion.response_model;
            result.input_tokens = completion.input_tokens;
            result.output_tokens = completion.output_tokens;
            result.reasoning_tokens = completion.reasoning_tokens;
        }
        Err(message) => result.error = Some(excerpt(&message)),
    }
    result
}

async fn execute(
    host: &HostClient,
    target: &Target<'_>,
) -> (Option<String>, Result<Completion, String>) {
    let request = ModelExecuteRequest {
        client_key_id: Some(target.key.id.clone()),
        model: target.model.to_owned(),
        protocol: PROVIDER.to_owned(),
        operation: ModelOperation::Generate,
        provider: Some(PROVIDER.to_owned()),
        account_id: Some(target.account.account_id.clone()),
        previous_response_id: None,
    };
    let body = request_body(target.model, &target.question.prompt, target.effort);
    // 宿主可能先在并发队列中等待再连接上游，这段时间同样计入单题上限。
    let deadline = Instant::now() + CHECK_TIMEOUT;
    let opened = before(deadline, async {
        host_calls::execute_stream(host, &request, &body)
            .await
            .map_err(|fault| fault.message)
    })
    .await;
    let stream = match opened {
        Ok(stream) => stream,
        Err(message) => return (None, Err(message)),
    };
    let read = collect_events(deadline, || {
        let id = stream.stream.as_str();
        async move {
            host_calls::stream_read(host, id)
                .await
                .map_err(|fault| fault.message)
        }
    })
    .await;
    let outcome = match read {
        Ok(events) => complete(&events),
        Err(message) => {
            // 关闭失败不改变结论：宿主会在父调用结束时回收未关闭的流。
            host_calls::stream_close(host, &stream.stream).await.ok();
            Err(message)
        }
    };
    (Some(stream.request_id), outcome)
}

#[cfg(test)]
mod tests {
    use super::{before, collect_events, request_body};
    use crate::results::Effort;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tokio::time::Instant;

    type Batch = Result<(Vec<Value>, bool, usize), String>;

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(150)
    }

    #[tokio::test(start_paused = true)]
    async fn collect_events_times_out() {
        let result = collect_events(deadline(), std::future::pending::<Batch>).await;
        assert_eq!(result.unwrap_err(), "检测超过 150 秒未完成");
    }

    #[tokio::test(start_paused = true)]
    async fn opening_the_stream_counts_against_the_deadline() {
        let result = before(deadline(), std::future::pending::<Result<(), String>>()).await;
        assert_eq!(result.unwrap_err(), "检测超过 150 秒未完成");
    }

    #[tokio::test(start_paused = true)]
    async fn reading_shares_the_remaining_time() {
        let started = Instant::now();
        let deadline = deadline();
        before(deadline, async {
            tokio::time::sleep(Duration::from_secs(100)).await;
            Ok(())
        })
        .await
        .unwrap();
        collect_events(deadline, std::future::pending::<Batch>)
            .await
            .unwrap_err();
        assert_eq!(started.elapsed(), Duration::from_secs(150));
    }

    #[tokio::test]
    async fn collect_events_caps_bytes() {
        let result =
            collect_events(deadline(), || async { Ok((Vec::new(), false, 600 * 1024)) }).await;
        assert_eq!(result.unwrap_err(), "模型响应超过 1 MiB");
    }

    #[tokio::test]
    async fn collect_events_stops_at_end() {
        let mut calls = 0;
        let result = collect_events(deadline(), || {
            calls += 1;
            let end = calls == 2;
            async move { Ok((vec![json!({ "n": 1 })], end, 10)) }
        })
        .await;
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn request_body_omits_reasoning_without_effort() {
        assert_eq!(
            request_body("m", "p", None),
            json!({
                "model": "m", "stream": true, "store": false, "instructions": "",
                "input": [{ "role": "user", "content": [{ "type": "input_text", "text": "p" }] }],
            })
        );
        assert_eq!(
            request_body("m", "p", Some(Effort::High))["reasoning"],
            json!({ "effort": "high" })
        );
    }
}

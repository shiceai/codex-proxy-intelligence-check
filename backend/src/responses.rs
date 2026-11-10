//! Responses 事件解析：从宿主模型流中取出 JSON 事件，并提取模型最终可见的答案。

use gateway_plugin_sdk::call::model::WirePayload;
use serde_json::Value;
use std::collections::BTreeMap;

/// 一次成功完成的响应中，判分与展示需要的信息。
#[derive(Debug, PartialEq)]
pub struct Completion {
    pub answer: String,
    pub response_model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

/// 取出一个模型事件的 JSON 内容。
///
/// OAuth 账号默认走上游 WebSocket，事件以 `WebSocket` 载荷交付；HTTP 上游以 `Json` 交付。
///
/// # 错误
///
/// 载荷类型不是流式事件时返回错误。
pub fn event_json(payload: &WirePayload) -> Result<Option<Value>, String> {
    match payload {
        WirePayload::Json { data, .. }
        | WirePayload::WebSocket {
            data: Some(data), ..
        } => Ok(Some(data.clone())),
        WirePayload::WebSocket {
            data: None,
            message,
            ..
        } => Ok(serde_json::from_slice(message).ok()),
        // 注释、心跳等非 JSON 帧不携带 Responses 事件；终态事件都是 JSON，跳过不影响判分。
        WirePayload::RawSse { .. } => Ok(None),
        _ => Err("不支持的模型事件载荷".to_owned()),
    }
}

/// 从完整事件序列中提取最终答案与用量。
///
/// # 错误
///
/// 响应未完成、上游失败、缺少完成事件或没有可见答案时返回错误说明。
pub fn complete(events: &[Value]) -> Result<Completion, String> {
    let mut items = BTreeMap::new();
    let mut terminal = None;
    for event in events {
        match event["type"].as_str() {
            Some("response.output_item.done") => {
                if let Some(index) = event["output_index"].as_u64() {
                    items.insert(index, &event["item"]);
                }
            }
            Some("response.completed") => terminal = Some(&event["response"]),
            Some("response.incomplete") => {
                let reason = &event["response"]["incomplete_details"]["reason"];
                return Err(format!("响应未完成：{}", text_or(reason, "未知原因")));
            }
            Some("response.failed") => {
                let message = &event["response"]["error"]["message"];
                return Err(format!("上游返回失败：{}", text_or(message, "未知错误")));
            }
            Some("error") => {
                return Err(format!(
                    "上游错误：{}",
                    text_or(&event["message"], "未知错误")
                ));
            }
            _ => {}
        }
    }
    let response = terminal.ok_or_else(|| "未收到 response.completed".to_owned())?;
    // Codex 上游常常只在 output_item.done 中交付内容，终态只保留身份和用量。
    let output: Vec<&Value> = match response["output"].as_array() {
        Some(output) if !output.is_empty() => output.iter().collect(),
        _ => items.into_values().collect(),
    };
    let answer = final_answer(&output);
    if answer.trim().is_empty() {
        return Err("未找到最终答案文本".to_owned());
    }
    let usage = &response["usage"];
    Ok(Completion {
        answer,
        response_model: response["model"].as_str().map(str::to_owned),
        input_tokens: usage["input_tokens"].as_u64(),
        output_tokens: usage["output_tokens"].as_u64(),
        reasoning_tokens: usage["output_tokens_details"]["reasoning_tokens"].as_u64(),
    })
}

/// 只取 assistant message；存在 final_answer 阶段时忽略 commentary 等其他阶段。
fn final_answer(output: &[&Value]) -> String {
    let messages: Vec<&Value> = output
        .iter()
        .copied()
        .filter(|item| item["type"] == "message" && item["role"] == "assistant")
        .collect();
    let finals: Vec<&Value> = messages
        .iter()
        .copied()
        .filter(|item| item["phase"] == "final_answer")
        .collect();
    let chosen = if finals.is_empty() { messages } else { finals };
    chosen
        .iter()
        .map(|item| message_text(item))
        .collect::<Vec<_>>()
        .join("\n")
}

fn message_text(item: &Value) -> String {
    item["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| part["type"] == "output_text")
        .filter_map(|part| part["text"].as_str())
        .collect()
}

fn text_or<'a>(value: &'a Value, fallback: &'a str) -> &'a str {
    value.as_str().unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::{complete, event_json};
    use gateway_plugin_sdk::call::model::WirePayload;
    use serde_json::{Value, json};

    fn message(text: &str, phase: Option<&str>) -> Value {
        let mut item = json!({
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": text }],
        });
        if let Some(phase) = phase {
            item["phase"] = json!(phase);
        }
        item
    }

    fn completed_with(output: Vec<Value>) -> Value {
        json!({
            "type": "response.completed",
            "response": {
                "model": "gpt-5.5",
                "output": output,
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 20,
                    "output_tokens_details": { "reasoning_tokens": 15 },
                },
            },
        })
    }

    fn item_done(index: u64, item: Value) -> Value {
        json!({ "type": "response.output_item.done", "output_index": index, "item": item })
    }

    fn incomplete(reason: &str) -> Value {
        json!({ "type": "response.incomplete", "response": { "incomplete_details": { "reason": reason } } })
    }

    fn failed(message: &str) -> Value {
        json!({ "type": "response.failed", "response": { "error": { "message": message } } })
    }

    #[test]
    fn final_answer_phase_wins_over_commentary() {
        let events = [completed_with(vec![
            message("思考一下", Some("commentary")),
            message("21", Some("final_answer")),
        ])];
        assert_eq!(complete(&events).unwrap().answer, "21");
    }

    #[test]
    fn empty_completed_output_falls_back_to_output_item_done() {
        let events = [
            item_done(1, message("21", None)),
            item_done(0, json!({ "type": "reasoning" })),
            completed_with(vec![]),
        ];
        assert_eq!(complete(&events).unwrap().answer, "21");
    }

    #[test]
    fn completed_output_wins_over_deltas() {
        let events = [
            json!({ "type": "response.output_text.delta", "delta": "99" }),
            completed_with(vec![message("21", None)]),
        ];
        assert_eq!(complete(&events).unwrap().answer, "21");
    }

    #[test]
    fn messages_without_phase_are_joined() {
        let events = [completed_with(vec![message("2", None), message("1", None)])];
        assert_eq!(complete(&events).unwrap().answer, "2\n1");
    }

    #[test]
    fn usage_and_model_are_read() {
        let completion = complete(&[completed_with(vec![message("21", None)])]).unwrap();
        assert_eq!(
            (
                completion.response_model.as_deref(),
                completion.input_tokens,
                completion.output_tokens,
                completion.reasoning_tokens,
            ),
            (Some("gpt-5.5"), Some(10), Some(20), Some(15))
        );
    }

    #[test]
    fn terminal_failures_are_errors() {
        assert_eq!(
            complete(&[incomplete("max_output_tokens")]).unwrap_err(),
            "响应未完成：max_output_tokens"
        );
        assert_eq!(
            complete(&[failed("server_error")]).unwrap_err(),
            "上游返回失败：server_error"
        );
        assert_eq!(
            complete(&[json!({ "type": "error", "message": "rate limited" })]).unwrap_err(),
            "上游错误：rate limited"
        );
        assert_eq!(complete(&[]).unwrap_err(), "未收到 response.completed");
        assert_eq!(
            complete(&[completed_with(vec![json!({ "type": "reasoning" })])]).unwrap_err(),
            "未找到最终答案文本"
        );
    }

    #[test]
    fn payload_kinds_are_decoded() {
        let data = json!({ "type": "response.created" });
        let sse = WirePayload::Json {
            event: None,
            data: data.clone(),
            id: None,
            retry: None,
            raw_sse: None,
        };
        assert_eq!(event_json(&sse).unwrap(), Some(data.clone()));

        let parsed = WirePayload::WebSocket {
            event: None,
            data: Some(data.clone()),
            message: Vec::new(),
        };
        assert_eq!(event_json(&parsed).unwrap(), Some(data));

        let raw = WirePayload::WebSocket {
            event: None,
            data: None,
            message: br#"{"type":"x"}"#.to_vec(),
        };
        assert_eq!(event_json(&raw).unwrap(), Some(json!({ "type": "x" })));

        let invalid = WirePayload::WebSocket {
            event: None,
            data: None,
            message: b"not json".to_vec(),
        };
        assert_eq!(event_json(&invalid).unwrap(), None);

        let comment = WirePayload::RawSse {
            frame: b": ping\n\n".to_vec(),
        };
        assert_eq!(event_json(&comment).unwrap(), None);

        let body = WirePayload::RawJson {
            body: b"{}".to_vec(),
        };
        assert_eq!(event_json(&body).unwrap_err(), "不支持的模型事件载荷");
    }
}

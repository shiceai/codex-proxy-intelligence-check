use gateway_plugin_sdk::{ErrorCode, call::host::ModelExecuteRequest};
use serde_json::{Value, json};

use crate::support::{Host, Peer, StateStore, account, batch, result, sample_result, ws_batch};

fn message(text: &str) -> Value {
    json!({
        "type": "message", "role": "assistant", "phase": "final_answer",
        "content": [{ "type": "output_text", "text": text }],
    })
}

/// Codex 常见形态：答案只在 output_item.done 中，completed 的 output 为空。
fn answer_events(answer: &str) -> Vec<Value> {
    vec![
        json!({ "type": "response.output_item.done", "output_index": 0, "item": message(answer) }),
        json!({ "type": "response.completed", "response": {
            "model": "gpt-6-astra", "output": [],
            "usage": { "input_tokens": 10, "output_tokens": 20, "output_tokens_details": { "reasoning_tokens": 15 } },
        }}),
    ]
}

fn task(account: &str, state: &str) -> Value {
    json!({"account_id":account,"question_id":"candy","state":state,"result":null,"note":null})
}

fn job(status: &str, tasks: Vec<Value>) -> Value {
    json!({
        "id": "j1", "created_at_ms": 1, "finished_at_ms": null,
        "key_id": "key_1", "key_name": "主 Key", "model": "gpt-6-astra", "effort": "high",
        "status": status, "tasks": tasks,
    })
}

/// 两个可用账号 a、b，分别回答 21 与 29。
fn host(job: Value) -> Host {
    let mut host = Host::new(StateStore::default().with("jobs", "current", job));
    for (id, answer) in [("a", "21"), ("b", "29")] {
        host.accounts.insert(
            id.to_owned(),
            account(id, "openai", true, Some(&format!("{id}@x.com")), id),
        );
        host.streams
            .insert(id.to_owned(), (2, batch(answer_events(answer))));
    }
    host
}

async fn reconcile(host: &mut Host) -> Value {
    let mut peer = Peer::start().await;
    let frame = peer.reconcile(|m, p, b| host.callback(m, p, b)).await;
    assert_eq!(result(&frame), &json!({}));
    host.state
        .get("jobs", "current")
        .cloned()
        .unwrap_or(json!(null))
}

#[tokio::test]
async fn reconcile_without_job_returns() {
    let mut host = Host::new(StateStore::default());
    assert_eq!(reconcile(&mut host).await, json!(null));
    assert_eq!(host.calls, ["host.state.get"]);
}

#[tokio::test]
async fn reconcile_runs_tasks_and_finishes() {
    let mut host = host(job(
        "running",
        vec![task("a", "pending"), task("b", "pending")],
    ));
    let job = reconcile(&mut host).await;
    assert_eq!(job["status"], "finished");
    assert!(job["finished_at_ms"].is_u64());
    assert_eq!(job["tasks"][0]["state"], "done");
    assert_eq!(job["tasks"][0]["result"]["verdict"], "normal");
    assert_eq!(job["tasks"][0]["result"]["saved"], true);
    assert_eq!(job["tasks"][1]["result"]["verdict"], "failed");
    let saved = host.state.get("results", "a").unwrap();
    assert_eq!(saved["items"][0]["id"], job["tasks"][0]["result"]["id"]);
    assert_eq!(saved["items"][0]["key_name"], "主 Key");

    let request: ModelExecuteRequest = serde_json::from_value(host.executed[0].clone()).unwrap();
    assert_eq!(request.account_id.as_deref(), Some("a"));
    assert_eq!(request.client_key_id.as_deref(), Some("key_1"));
    assert_eq!(request.model, "gpt-6-astra");
    assert_eq!(host.executed.len(), 2);
}

#[tokio::test]
async fn reconcile_marks_leftover_running_as_interrupted() {
    let mut host = host(job(
        "running",
        vec![task("a", "running"), task("b", "pending")],
    ));
    let job = reconcile(&mut host).await;
    assert_eq!(job["tasks"][0]["state"], "interrupted");
    assert_eq!(
        job["tasks"][0]["note"],
        "检测被中断：插件停用、升级或宿主重启"
    );
    assert_eq!(job["tasks"][1]["state"], "done");
    assert_eq!(host.count("host.model.execute_stream"), 1);
    assert_eq!(job["status"], "finished");
}

#[tokio::test]
async fn reconcile_stopping_skips_pending() {
    let mut host = host(job(
        "stopping",
        vec![task("a", "pending"), task("b", "pending")],
    ));
    let job = reconcile(&mut host).await;
    assert_eq!(job["status"], "finished");
    assert_eq!(job["tasks"][0]["state"], "skipped");
    assert_eq!(job["tasks"][1]["note"], "已停止");
    assert_eq!(host.count("host.model.execute_stream"), 0);
}

#[tokio::test]
async fn reconcile_skips_unavailable_account_without_model_call() {
    let mut host = host(job(
        "running",
        vec![task("a", "pending"), task("b", "pending")],
    ));
    host.accounts
        .insert("a".to_owned(), account("a", "openai", false, None, "a"));
    let job = reconcile(&mut host).await;
    assert_eq!(job["tasks"][0]["state"], "skipped");
    assert_eq!(
        job["tasks"][0]["note"],
        "账号不存在、已停用或不是 OpenAI 账号"
    );
    assert_eq!(job["tasks"][1]["state"], "done");
    assert_eq!(host.executed.len(), 1);
}

#[tokio::test]
async fn reconcile_skips_disabled_question() {
    let mut host = host(job("running", vec![task("a", "pending")]));
    host.state = StateStore::default()
        .with("jobs", "current", job("running", vec![task("a", "pending")]))
        .with(
            "questions",
            "bank",
            json!({"items":[{"id":"candy","name":"糖果配对","prompt":"x","expected":"21","match":"exact","enabled":false,"builtin":true}]}),
        );
    let job = reconcile(&mut host).await;
    assert_eq!(job["tasks"][0]["state"], "skipped");
    assert_eq!(job["tasks"][0]["note"], "题目不存在或已停用");
}

#[tokio::test]
async fn reconcile_records_host_rejection_as_error_verdict() {
    let mut host = host(job("running", vec![task("a", "pending")]));
    host.execute_error = Some("账号不在 Key 的分组范围内");
    let job = reconcile(&mut host).await;
    assert_eq!(job["tasks"][0]["state"], "done");
    assert_eq!(job["tasks"][0]["result"]["verdict"], "error");
    let saved = host.state.get("results", "a").unwrap();
    assert!(
        saved["items"][0]["error"]
            .as_str()
            .unwrap()
            .contains("账号不在 Key 的分组范围内")
    );
}

#[tokio::test]
async fn reconcile_parses_websocket_payloads() {
    let mut host = host(job("running", vec![task("a", "pending")]));
    host.streams
        .insert("a".to_owned(), (2, ws_batch(answer_events("21"))));
    let job = reconcile(&mut host).await;
    assert_eq!(job["tasks"][0]["result"]["verdict"], "normal");
}

#[tokio::test]
async fn reconcile_reports_unsaved_result() {
    let mut host = host(job("running", vec![task("a", "pending")]));
    host.state
        .failing_puts
        .push(("results".to_owned(), ErrorCode::Fault));
    let job = reconcile(&mut host).await;
    assert_eq!(job["tasks"][0]["result"]["saved"], false);
    assert_eq!(job["tasks"][0]["note"], "结果写入失败，未保存");
}

#[tokio::test]
async fn reconcile_retries_result_conflict() {
    // 写入结果前，另一个写入者刚给该账号存了一条：重读后两条都要保留。
    let mut host = host(job("running", vec![task("a", "pending")]));
    host.state
        .race_next_put("results", "a", json!({ "items": [sample_result("r0")] }));
    let job = reconcile(&mut host).await;
    assert_eq!(job["tasks"][0]["result"]["saved"], true);
    let items = host.state.get("results", "a").unwrap()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(items.len(), 2);
    assert_eq!(items[1]["id"], "r0");
}

#[tokio::test]
async fn reconcile_finishes_in_flight_tasks_when_a_progress_write_fails() {
    // 两道题都已发出；记录第一道完成时任务写入失败：第二道仍要跑完，两条结果都要记下。
    let mut host = host(job(
        "running",
        vec![task("a", "pending"), task("b", "pending")],
    ));
    host.state.fail_nth_put = Some(("jobs".to_owned(), 3));
    let job = reconcile(&mut host).await;
    assert!(host.state.get("results", "a").is_some());
    assert!(host.state.get("results", "b").is_some());
    assert_eq!(job["tasks"][0]["state"], "done");
    assert_eq!(job["tasks"][1]["state"], "done");
    assert_eq!(job["status"], "finished");
}

#[tokio::test]
async fn reconcile_keeps_running_task_when_starting_another_fails() {
    // 第二道题标记开始时写入失败：已发出的第一道题不能被丢弃。
    let mut host = host(job(
        "running",
        vec![task("a", "pending"), task("b", "pending")],
    ));
    host.state.fail_nth_put = Some(("jobs".to_owned(), 2));
    let mut peer = Peer::start().await;
    let frame = peer.reconcile(|m, p, b| host.callback(m, p, b)).await;
    assert!(
        matches!(frame.message, gateway_plugin_sdk::Message::Error { .. }),
        "本轮应报错让宿主稍后重试：{frame:?}"
    );
    assert!(host.state.get("results", "a").is_some());
    let job = host.state.get("jobs", "current").unwrap();
    assert_eq!(job["tasks"][0]["state"], "done");
    assert_eq!(job["tasks"][1]["state"], "pending");
}

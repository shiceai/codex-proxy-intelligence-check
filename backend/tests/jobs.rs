use serde_json::{Value, json};

use crate::support::{Host, Peer, StateStore, no_callback};

fn question(id: &str, enabled: bool, builtin: bool) -> Value {
    json!({"id":id,"name":"题目","prompt":"x","expected":"21","match":"exact","enabled":enabled,"builtin":builtin})
}

fn bank(items: Vec<Value>) -> Value {
    json!({ "items": items })
}

fn job(id: &str, status: &str) -> Value {
    json!({
        "id": id, "created_at_ms": 1, "finished_at_ms": null,
        "key_id": "key_1", "key_name": "主 Key", "model": "gpt-6-astra", "effort": "high",
        "status": status,
        "tasks": [
            {"account_id":"a","question_id":"candy","state":"pending","result":null,"note":null},
            {"account_id":"b","question_id":"candy","state":"pending","result":null,"note":null},
        ],
    })
}

fn submit() -> Value {
    json!({"account_ids":["a","b"],"key_id":"key_1","model":"gpt-6-astra","effort":"high"})
}

async fn api(host: &mut Host, method: &str, path: &str, data: Option<Value>) -> (u64, Value) {
    let mut peer = Peer::start().await;
    peer.api(method, path, data, |m, p, b| host.callback(m, p, b))
        .await
}

#[tokio::test]
async fn submit_creates_job_from_enabled_questions() {
    let state = StateStore::default().with(
        "questions",
        "bank",
        bank(vec![
            question("candy", true, true),
            question("q_1_1", true, false),
            question("q_1_2", false, false),
        ]),
    );
    let mut host = Host::new(state);
    let (status, body) = api(&mut host, "POST", "api/jobs", Some(submit())).await;
    assert_eq!(status, 200, "{body}");
    let order: Vec<String> = body["job"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| {
            format!(
                "{}:{}",
                task["account_id"].as_str().unwrap(),
                task["question_id"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(order, ["a:candy", "a:q_1_1", "b:candy", "b:q_1_1"]);
    assert_eq!(body["job"]["key_name"], "主 Key");
    assert_eq!(body["job"]["status"], "running");
    assert_eq!(host.state.get("jobs", "current"), Some(&body["job"]));
    assert_eq!(host.count("host.keys.list"), 1);
}

#[tokio::test]
async fn submit_rejects_when_job_active() {
    let state = StateStore::default().with("jobs", "current", job("j1", "running"));
    let mut host = Host::new(state);
    let response = api(&mut host, "POST", "api/jobs", Some(submit())).await;
    assert_eq!(
        response,
        (
            409,
            json!({"error":{"code":"job_active","message":"已有检测在进行，请等待完成或先停止"}})
        )
    );
}

#[tokio::test]
async fn submit_replaces_finished_job() {
    let state = StateStore::default().with("jobs", "current", job("j1", "finished"));
    let mut host = Host::new(state);
    let (status, body) = api(&mut host, "POST", "api/jobs", Some(submit())).await;
    assert_eq!(status, 200, "{body}");
    assert_ne!(body["job"]["id"], "j1");
}

#[tokio::test]
async fn submit_requires_enabled_key() {
    let mut host = Host::new(StateStore::default());
    host.keys = vec![json!({"id":"key_1","name":"主 Key","enabled":false})];
    let response = api(&mut host, "POST", "api/jobs", Some(submit())).await;
    assert_eq!(
        response,
        (
            400,
            json!({"error":{"code":"invalid_request","message":"Key 不存在或已停用"}})
        )
    );
}

#[tokio::test]
async fn submit_requires_enabled_questions() {
    let state = StateStore::default().with(
        "questions",
        "bank",
        bank(vec![question("candy", false, true)]),
    );
    let mut host = Host::new(state);
    let response = api(&mut host, "POST", "api/jobs", Some(submit())).await;
    assert_eq!(
        response,
        (
            400,
            json!({"error":{"code":"invalid_request","message":"题库中没有启用的题目"}})
        )
    );
}

#[tokio::test]
async fn current_returns_job_or_null() {
    let mut host = Host::new(StateStore::default());
    assert_eq!(
        api(&mut host, "GET", "api/jobs/current", None).await,
        (200, json!({"job":null}))
    );
    let mut host = Host::new(StateStore::default().with("jobs", "current", job("j1", "running")));
    let (status, body) = api(&mut host, "GET", "api/jobs/current", None).await;
    assert_eq!((status, &body["job"]["id"]), (200, &json!("j1")));
}

#[tokio::test]
async fn stop_skips_pending_and_waits_for_running() {
    let mut running = job("j1", "running");
    running["tasks"][0]["state"] = json!("running");
    let mut host = Host::new(StateStore::default().with("jobs", "current", running));
    let (status, body) = api(
        &mut host,
        "POST",
        "api/jobs/stop",
        Some(json!({"job_id":"j1"})),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["job"]["status"], "stopping");
    assert_eq!(body["job"]["tasks"][1]["state"], "skipped");
    assert_eq!(
        host.state.get("jobs", "current").unwrap()["status"],
        "stopping"
    );
}

#[tokio::test]
async fn stop_with_nothing_running_finishes_immediately() {
    // 后台没有在跑的题时直接结束，不必等下一轮后台调用，也不会卡住后续提交。
    let mut host = Host::new(StateStore::default().with("jobs", "current", job("j1", "running")));
    let (status, body) = api(
        &mut host,
        "POST",
        "api/jobs/stop",
        Some(json!({"job_id":"j1"})),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["job"]["status"], "finished");
    assert_eq!(body["job"]["tasks"][0]["note"], "已停止");
}

#[tokio::test]
async fn stop_rejects_other_job_id() {
    let mut host = Host::new(StateStore::default().with("jobs", "current", job("j2", "running")));
    let response = api(
        &mut host,
        "POST",
        "api/jobs/stop",
        Some(json!({"job_id":"j1"})),
    )
    .await;
    assert_eq!(
        response,
        (
            409,
            json!({"error":{"code":"job_changed","message":"检测任务已变化，请刷新页面"}})
        )
    );
}

#[tokio::test]
async fn stop_keeps_concurrent_progress() {
    // 停止写入前，后台刚开始第一题：重读重放后两边的修改都要保留。
    let mut host = Host::new(StateStore::default().with("jobs", "current", job("j1", "running")));
    let mut progressed = job("j1", "running");
    progressed["tasks"][0]["state"] = json!("running");
    host.state.race_next_put("jobs", "current", progressed);
    let (status, body) = api(
        &mut host,
        "POST",
        "api/jobs/stop",
        Some(json!({"job_id":"j1"})),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let stored = host.state.get("jobs", "current").unwrap();
    assert_eq!(stored["status"], "stopping");
    assert_eq!(stored["tasks"][0]["state"], "running");
    assert_eq!(stored["tasks"][1]["state"], "skipped");
}

#[tokio::test]
async fn submit_validates_ids() {
    let data = json!({"account_ids":[""],"key_id":"key_1","model":"gpt-6-astra","effort":null});
    let (status, _) = Peer::start()
        .await
        .api("POST", "api/jobs", Some(data), no_callback)
        .await;
    assert_eq!(status, 400);
}

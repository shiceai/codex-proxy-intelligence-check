use gateway_plugin_sdk::{ErrorCode, PluginFault};
use serde_json::{Value, json};

use crate::support::Peer;

fn candy() -> Value {
    json!({"id":"candy","name":"糖果配对","prompt":"x","expected":"21","match":"exact","enabled":true,"builtin":true})
}

fn new_question() -> Value {
    json!({"id":"","name":"加法","prompt":"1+1=?","expected":"2","match":"exact","enabled":true,"builtin":false})
}

#[tokio::test]
async fn save_questions_writes_merged_bank_with_expected_version() {
    let mut peer = Peer::start().await;
    let mut written = None;
    let (status, body) = peer
        .api(
            "POST",
            "api/questions",
            Some(json!({"version":3,"items":[candy(), new_question()]})),
            |method, params, _| match method {
                "host.state.get" => {
                    assert_eq!(params, &json!({"namespace":"questions","key":"bank"}));
                    Ok((
                        json!({"record":{"schema_version":1,"version":3,"value":{"items":[candy()]}}}),
                        Vec::new(),
                    ))
                }
                "host.state.put" => {
                    assert_eq!(params["namespace"], "questions");
                    assert_eq!(params["key"], "bank");
                    assert_eq!(params["expected_version"], 3);
                    written = Some(params["value"].clone());
                    Ok((json!({"version":4}), Vec::new()))
                }
                _ => panic!("出现未预期的宿主回调：{method}"),
            },
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["version"], 4);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items[0]["id"], "candy");
    assert!(
        items[0]["prompt"]
            .as_str()
            .unwrap()
            .starts_with("不允许调用工具和联网")
    );
    assert!(items[1]["id"].as_str().unwrap().starts_with("q_"));
    assert_eq!(written.unwrap()["items"], body["items"]);
}

#[tokio::test]
async fn save_questions_conflict_returns_409() {
    let mut peer = Peer::start().await;
    let response = peer
        .api(
            "POST",
            "api/questions",
            Some(json!({"version":3,"items":[candy()]})),
            // 读取时版本一致，写入前被其他页面抢先保存：由宿主 CAS 拒绝。
            |method, _, _| match method {
                "host.state.get" => Ok((
                    json!({"record":{"schema_version":1,"version":3,"value":{"items":[candy()]}}}),
                    Vec::new(),
                )),
                "host.state.put" => Err(PluginFault::new(ErrorCode::Conflict, "version mismatch")),
                _ => panic!("出现未预期的宿主回调：{method}"),
            },
        )
        .await;
    assert_eq!(
        response,
        (
            409,
            json!({"error":{"code":"conflict","message":"题库已被修改，请刷新后重试"}})
        )
    );
}

#[tokio::test]
async fn save_questions_rejects_removing_builtin() {
    let mut peer = Peer::start().await;
    let response = peer
        .api(
            "POST",
            "api/questions",
            Some(json!({"version":null,"items":[new_question()]})),
            |method, _, _| match method {
                "host.state.get" => Ok((json!({"record":null}), Vec::new())),
                _ => panic!("出现未预期的宿主回调：{method}"),
            },
        )
        .await;
    assert_eq!(
        response,
        (
            400,
            json!({"error":{"code":"invalid_request","message":"内置题不能删除"}})
        )
    );
}

#[tokio::test]
async fn save_questions_accepts_full_bank_of_long_prompts() {
    let mut peer = Peer::start().await;
    let long: Vec<Value> = (0..19)
        .map(|index| {
            json!({"id":"","name":format!("长题{index}"),"prompt":"糖".repeat(2000),
                   "expected":"2","match":"exact","enabled":true,"builtin":false})
        })
        .collect();
    let items: Vec<Value> = std::iter::once(candy()).chain(long).collect();
    let (status, body) = peer
        .api(
            "POST",
            "api/questions",
            Some(json!({"version":null,"items":items})),
            |method, _, _| match method {
                "host.state.get" => Ok((json!({"record":null}), Vec::new())),
                "host.state.put" => Ok((json!({"version":1}), Vec::new())),
                _ => panic!("出现未预期的宿主回调：{method}"),
            },
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 20);
}

#[tokio::test]
async fn save_questions_with_stale_version_returns_409_before_validation() {
    let mut peer = Peer::start().await;
    // 另一个页面已删除 q_1_0 并保存为版本 5；本页仍按版本 4 提交含 q_1_0 的题库。
    let stale = json!({"id":"q_1_0","name":"加法","prompt":"1+1=?","expected":"2","match":"exact","enabled":true,"builtin":false});
    let response = peer
        .api(
            "POST",
            "api/questions",
            Some(json!({"version":4,"items":[candy(), stale]})),
            |method, _, _| match method {
                "host.state.get" => Ok((
                    json!({"record":{"schema_version":1,"version":5,"value":{"items":[candy()]}}}),
                    Vec::new(),
                )),
                _ => panic!("出现未预期的宿主回调：{method}"),
            },
        )
        .await;
    assert_eq!(
        response,
        (
            409,
            json!({"error":{"code":"conflict","message":"题库已被修改，请刷新后重试"}})
        )
    );
}

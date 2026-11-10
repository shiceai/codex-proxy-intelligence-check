use serde_json::json;

use crate::support::{Peer, account, no_callback, sample_result};

#[tokio::test]
async fn results_route_reads_account_record() {
    let mut peer = Peer::start().await;
    let response = peer
        .api(
            "POST",
            "api/results",
            Some(json!({"account_id":"acct_1"})),
            |method, params, _| {
                assert_eq!(method, "host.state.get");
                assert_eq!(params, &json!({"namespace":"results","key":"acct_1"}));
                Ok((
                    json!({"record":{"schema_version":1,"version":2,"value":{"items":[sample_result("r1")]}}}),
                    Vec::new(),
                ))
            },
        )
        .await;
    assert_eq!(response, (200, json!({"items":[sample_result("r1")]})));
}

#[tokio::test]
async fn results_route_returns_empty_when_absent() {
    let mut peer = Peer::start().await;
    let response = peer
        .api(
            "POST",
            "api/results",
            Some(json!({"account_id":"acct_1"})),
            |_, _, _| Ok((json!({"record":null}), Vec::new())),
        )
        .await;
    assert_eq!(response, (200, json!({"items":[]})));
}

#[tokio::test]
async fn results_route_requires_account_id() {
    let mut peer = Peer::start().await;
    let (status, _) = peer
        .api(
            "POST",
            "api/results",
            Some(json!({"account_id":""})),
            no_callback,
        )
        .await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn bootstrap_lists_enabled_openai_accounts_keys_and_bank() {
    let mut peer = Peer::start().await;
    let (status, body) = peer
        .api(
            "GET",
            "api/bootstrap",
            None,
            |method, params, payload| match method {
                "host.auth.list" => {
                    assert_eq!(params, &json!({}));
                    let request: serde_json::Value = serde_json::from_slice(payload).unwrap();
                    assert_eq!(request["provider_id"], "openai");
                    assert_eq!(request["limit"], 100);
                    let page = if request["cursor"].is_null() {
                        json!({"accounts":[
                            account("acct_b", "openai", true, Some(" b@x.com "), "Bravo"),
                            account("acct_off", "openai", false, None, "Off"),
                        ],"next_cursor":"c1"})
                    } else {
                        assert_eq!(request["cursor"], "c1");
                        json!({"accounts":[
                            account("acct_a", "openai", true, None, "Alpha"),
                            account("acct_xai", "xai", true, None, "Xai"),
                        ],"next_cursor":null})
                    };
                    Ok((json!({}), serde_json::to_vec(&page).unwrap()))
                }
                "host.keys.list" => Ok((
                    json!({"keys":[
                        {"id":"key_1","name":"主 Key","enabled":true},
                        {"id":"key_2","name":"停用","enabled":false},
                    ],"next_cursor":null}),
                    Vec::new(),
                )),
                "host.state.get" => {
                    let record = match (params["namespace"].as_str(), params["key"].as_str()) {
                        (Some("questions"), Some("bank"))
                        | (Some("results"), Some("acct_b"))
                        | (Some("jobs"), Some("current")) => json!(null),
                        (Some("results"), Some("acct_a")) => json!({
                            "schema_version":1,"version":1,"value":{"items":[sample_result("r1")]}
                        }),
                        other => panic!("未预期的状态读取：{other:?}"),
                    };
                    Ok((json!({ "record": record }), Vec::new()))
                }
                _ => panic!("出现未预期的宿主回调：{method}"),
            },
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["accounts"],
        json!([
            {"id":"acct_a","label":"Alpha","plan_type":"plus","recent":[
                {"id":"r1","verdict":"normal","created_at_ms":1_700_000_000_000_u64}
            ]},
            {"id":"acct_b","label":"b@x.com","plan_type":"plus","recent":[]},
        ])
    );
    assert_eq!(body["keys"], json!([{"id":"key_1","name":"主 Key"}]));
    assert_eq!(body["questions"]["version"], json!(null));
    assert_eq!(body["questions"]["items"][0]["id"], "candy");
    assert_eq!(body.get("job"), Some(&json!(null)));
}

#[tokio::test]
async fn models_route_queries_selected_key() {
    let mut peer = Peer::start().await;
    let response = peer
        .api(
            "POST",
            "api/models",
            Some(json!({"key_id":"key_1"})),
            |method, params, _| {
                assert_eq!(method, "host.models.list");
                assert_eq!(
                    params,
                    &json!({"client_key_id":"key_1","protocol":"openai","client_version":"intelligence-check/1.0.0"})
                );
                Ok((json!({"models":["gpt-5.5"]}), Vec::new()))
            },
        )
        .await;
    assert_eq!(response, (200, json!({"models":["gpt-5.5"]})));
}

#[tokio::test]
async fn models_route_requires_key_id() {
    let mut peer = Peer::start().await;
    let (status, _) = peer
        .api(
            "POST",
            "api/models",
            Some(json!({"key_id":""})),
            no_callback,
        )
        .await;
    assert_eq!(status, 400);
}

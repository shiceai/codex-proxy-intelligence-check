use std::time::Duration;

use gateway_plugin_sdk::Stage;
use serde_json::json;

use crate::support::{Peer, no_callback};

#[tokio::test]
async fn ready_declares_maintenance_budget() {
    let peer = Peer::start().await;
    assert_eq!(
        peer.budgets.budget(Stage::Maintenance),
        Duration::from_secs(30 * 60)
    );
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let mut peer = Peer::start().await;
    let response = peer.api("GET", "api/nope", None, no_callback).await;
    assert_eq!(
        response,
        (
            404,
            json!({"error":{"code":"not_found","message":"未找到插件管理接口"}})
        )
    );
}

#[tokio::test]
async fn query_string_is_rejected() {
    let mut peer = Peer::start().await;
    let response = peer
        .api_with_query("GET", "api/bootstrap", "a=1", None, no_callback)
        .await;
    assert_eq!(
        response,
        (
            400,
            json!({"error":{"code":"invalid_request","message":"此接口不支持查询参数"}})
        )
    );
}

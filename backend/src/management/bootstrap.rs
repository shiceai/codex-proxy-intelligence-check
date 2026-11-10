use super::{
    response::{ApiError, ApiResult, json_reply},
    validation::{bounded, decode_json, require_empty_body},
};
use crate::{
    host_calls,
    jobs::{self, Job},
    questions::{self, Bank},
    results::{self, Recent},
};
use gateway_plugin_sdk::{call::management::ManagementRequest, client::TypedCall};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize)]
struct Bootstrap {
    accounts: Vec<AccountView>,
    keys: Vec<KeyView>,
    questions: Bank,
    job: Option<Job>,
}

#[derive(Serialize)]
struct AccountView {
    id: String,
    label: String,
    plan_type: Option<String>,
    recent: Vec<Recent>,
}

#[derive(Serialize)]
struct KeyView {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelsRequest {
    key_id: String,
}

/// 页面首屏数据：账号与最近结果摘要、可选 Key、题库。
pub(super) async fn bootstrap(call: TypedCall<ManagementRequest>) -> ApiResult {
    require_empty_body(&call.request, &call.payload)?;
    let mut accounts = Vec::new();
    for account in host_calls::openai_accounts(&call.host).await? {
        let items = results::list(&call.host, &account.account_id).await?;
        accounts.push(AccountView {
            label: host_calls::account_label(&account),
            id: account.account_id,
            plan_type: account.plan_type,
            recent: results::recent(&items),
        });
    }
    accounts.sort_by(|left, right| left.label.cmp(&right.label));
    let keys = host_calls::enabled_keys(&call.host)
        .await?
        .into_iter()
        .map(|key| KeyView {
            id: key.id,
            name: key.name,
        })
        .collect();
    let questions = questions::load(&call.host).await?;
    let job = jobs::load(&call.host).await?.map(|(_, job)| job);
    json_reply(&Bootstrap {
        accounts,
        keys,
        questions,
        job,
    })
}

pub(super) async fn models(call: TypedCall<ManagementRequest>) -> ApiResult {
    let request: ModelsRequest = decode_json(&call)?;
    if !bounded(&request.key_id, 256) {
        return Err(ApiError::invalid("Key ID 无效"));
    }
    let models = host_calls::models(&call.host, &request.key_id).await?;
    json_reply(&json!({ "models": models }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultsRequest {
    account_id: String,
}

/// 单个账号最近 20 条完整结果，供详情对话框使用。
pub(super) async fn results(call: TypedCall<ManagementRequest>) -> ApiResult {
    let request: ResultsRequest = decode_json(&call)?;
    if !bounded(&request.account_id, 256) {
        return Err(ApiError::invalid("账号 ID 无效"));
    }
    let items = results::list(&call.host, &request.account_id).await?;
    json_reply(&json!({ "items": items }))
}

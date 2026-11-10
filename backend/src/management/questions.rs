use super::{
    response::{ApiError, ApiResult, json_reply},
    validation::decode_json,
};
use crate::{
    now_ms,
    questions::{self, Bank, Question},
};
use gateway_plugin_sdk::{ErrorCode, call::management::ManagementRequest, client::TypedCall};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    version: Option<u64>,
    items: Vec<Question>,
}

/// 整表替换题库；版本号不匹配说明其他页面已经保存过。
pub(super) async fn save(call: TypedCall<ManagementRequest>) -> ApiResult {
    let request: SaveRequest = decode_json(&call)?;
    let current = questions::load(&call.host).await?;
    // 先比较版本：其他页面增删过题目时，按旧列表校验会得到误导性的"题目不存在"。
    if request.version != current.version {
        return Err(conflict());
    }
    let items =
        questions::merge(&current.items, request.items, now_ms()).map_err(ApiError::invalid)?;
    let version = questions::save(&call.host, &items, request.version)
        .await
        .map_err(|fault| {
            if fault.code == ErrorCode::Conflict {
                conflict()
            } else {
                fault.into()
            }
        })?;
    json_reply(&Bank {
        version: Some(version),
        items,
    })
}

fn conflict() -> ApiError {
    ApiError::new(409, "conflict", "题库已被修改，请刷新后重试")
}

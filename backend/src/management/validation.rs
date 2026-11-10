use super::response::{ApiError, JSON_CONTENT_TYPE};
use gateway_plugin_sdk::{call::management::ManagementRequest, client::TypedCall};
use serde::de::DeserializeOwned;

pub(super) fn require_empty_body(
    request: &ManagementRequest,
    payload: &[u8],
) -> Result<(), ApiError> {
    if request.content_type.is_none() && payload.is_empty() {
        Ok(())
    } else {
        Err(ApiError::invalid("此接口不接受请求正文"))
    }
}

pub(super) fn decode_json<T: DeserializeOwned>(
    call: &TypedCall<ManagementRequest>,
) -> Result<T, ApiError> {
    require_json(&call.request)?;
    serde_json::from_slice(&call.payload).map_err(|_| ApiError::invalid("请求正文不是有效 JSON"))
}

fn require_json(request: &ManagementRequest) -> Result<(), ApiError> {
    if request
        .content_type
        .as_deref()
        .and_then(|content_type| content_type.split(';').next())
        .is_some_and(|content_type| content_type.trim().eq_ignore_ascii_case(JSON_CONTENT_TYPE))
    {
        Ok(())
    } else {
        Err(ApiError::new(
            400,
            "invalid_content_type",
            "此接口需要 application/json 内容类型",
        ))
    }
}

/// 按字符计数的非空标识，不允许控制字符。
pub(super) fn bounded(value: &str, maximum: usize) -> bool {
    (1..=maximum).contains(&value.chars().count()) && !value.chars().any(char::is_control)
}

use gateway_plugin_sdk::{
    ErrorCode, PluginFault, call::management::ManagementResponse, client::TypedReply,
};
use serde::Serialize;
use serde_json::json;

pub(super) const JSON_CONTENT_TYPE: &str = "application/json";
pub(super) type ApiResult = Result<TypedReply<ManagementResponse>, ApiError>;

/// 页面可见的错误；序列化为 `{"error":{"code","message"}}`。
#[derive(Serialize)]
pub(super) struct ApiError {
    #[serde(skip)]
    status: u16,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(400, "invalid_request", message)
    }

    pub fn into_reply(self) -> Result<TypedReply<ManagementResponse>, PluginFault> {
        encode(self.status, &json!({ "error": self }))
    }
}

impl From<PluginFault> for ApiError {
    fn from(error: PluginFault) -> Self {
        if error.code == ErrorCode::Conflict {
            Self::new(409, "conflict", "记录已更新，请刷新后重试")
        } else {
            Self::new(
                502,
                "host_callback",
                format!("宿主操作失败：{}", error.message),
            )
        }
    }
}

pub(super) fn json_reply(body: &impl Serialize) -> ApiResult {
    encode(200, body).map_err(|_| ApiError::new(500, "encoding", "管理接口响应编码失败"))
}

fn encode(
    status: u16,
    body: &impl Serialize,
) -> Result<TypedReply<ManagementResponse>, PluginFault> {
    let payload = serde_json::to_vec(body)
        .map_err(|_| PluginFault::new(ErrorCode::Fault, "管理接口响应编码失败"))?;
    Ok(TypedReply::new(ManagementResponse {
        headers: Vec::new(),
        status,
        content_type: JSON_CONTENT_TYPE.to_owned(),
    })
    .with_payload(payload))
}

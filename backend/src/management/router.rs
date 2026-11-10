use super::{
    bootstrap, jobs, questions,
    response::{ApiError, ApiResult},
};
use gateway_plugin_sdk::{
    PluginFault,
    call::management::{ManagementRequest, ManagementResponse},
    client::{TypedCall, TypedReply},
};

/// 20 道 2000 字的中文题库约 140 KB；宿主对管理请求正文的上限是 1 MiB。
const MAXIMUM_BODY_BYTES: usize = 512 * 1024;

pub(crate) async fn handle(
    call: TypedCall<ManagementRequest>,
) -> Result<TypedReply<ManagementResponse>, PluginFault> {
    route(call).await.or_else(ApiError::into_reply)
}

async fn route(call: TypedCall<ManagementRequest>) -> ApiResult {
    if !call.request.query.is_empty() {
        return Err(ApiError::invalid("此接口不支持查询参数"));
    }
    if call.payload.len() > MAXIMUM_BODY_BYTES {
        return Err(ApiError::invalid("请求正文过大"));
    }
    match (call.request.method.as_str(), call.request.path.as_str()) {
        ("GET", "api/bootstrap") => bootstrap::bootstrap(call).await,
        ("POST", "api/models") => bootstrap::models(call).await,
        ("POST", "api/results") => bootstrap::results(call).await,
        ("POST", "api/questions") => questions::save(call).await,
        ("GET", "api/jobs/current") => jobs::current(call).await,
        ("POST", "api/jobs") => jobs::submit(call).await,
        ("POST", "api/jobs/stop") => jobs::stop(call).await,
        _ => Err(ApiError::new(404, "not_found", "未找到插件管理接口")),
    }
}

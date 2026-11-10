use super::{
    response::{ApiError, ApiResult, json_reply},
    validation::{bounded, decode_json, require_empty_body},
};
use crate::{
    host_calls,
    jobs::{self, Job, JobStatus, NewJob},
    now_ms, questions,
    results::Effort,
};
use gateway_plugin_sdk::{ErrorCode, call::management::ManagementRequest, client::TypedCall};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitRequest {
    account_ids: Vec<String>,
    key_id: String,
    model: String,
    effort: Option<Effort>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StopRequest {
    job_id: String,
}

#[derive(Serialize)]
struct JobReply {
    job: Option<Job>,
}

pub(super) async fn current(call: TypedCall<ManagementRequest>) -> ApiResult {
    require_empty_body(&call.request, &call.payload)?;
    let job = jobs::load(&call.host).await?.map(|(_, job)| job);
    json_reply(&JobReply { job })
}

/// 提交一批检测；同一时间只允许一批，由后台维护任务逐题执行。
pub(super) async fn submit(call: TypedCall<ManagementRequest>) -> ApiResult {
    let request: SubmitRequest = decode_json(&call)?;
    if !bounded(&request.key_id, 256)
        || !bounded(&request.model, 256)
        || !request.account_ids.iter().all(|id| bounded(id, 256))
    {
        return Err(ApiError::invalid("检测参数无效"));
    }
    let existing = jobs::load(&call.host).await?;
    if existing
        .as_ref()
        .is_some_and(|(_, job)| jobs::is_active(job))
    {
        return Err(job_active());
    }
    let key = host_calls::enabled_keys(&call.host)
        .await?
        .into_iter()
        .find(|key| key.id == request.key_id)
        .ok_or_else(|| ApiError::invalid("Key 不存在或已停用"))?;
    let question_ids: Vec<String> = questions::load(&call.host)
        .await?
        .items
        .into_iter()
        .filter(|question| question.enabled)
        .map(|question| question.id)
        .collect();
    let job = jobs::build(
        now_ms(),
        NewJob {
            key_id: key.id,
            key_name: key.name,
            model: request.model,
            effort: request.effort,
        },
        &request.account_ids,
        &question_ids,
    )
    .map_err(ApiError::invalid)?;
    jobs::create(&call.host, &job, existing.map(|(version, _)| version))
        .await
        .map_err(|fault| {
            if fault.code == ErrorCode::Conflict {
                job_active()
            } else {
                fault.into()
            }
        })?;
    json_reply(&JobReply { job: Some(job) })
}

/// 请求停止：后台不再开始新题，进行中的题照常完成。
pub(super) async fn stop(call: TypedCall<ManagementRequest>) -> ApiResult {
    let request: StopRequest = decode_json(&call)?;
    let job = jobs::update(&call.host, |job| {
        let stop = job.id == request.job_id && job.status == JobStatus::Running;
        if stop {
            job.status = JobStatus::Stopping;
            // 剩余题立即标为已停止；没有进行中的题时直接结束，不必等下一轮后台调用。
            jobs::settle(job, now_ms());
        }
        stop
    })
    .await?
    .filter(|job| job.id == request.job_id)
    .ok_or_else(|| ApiError::new(409, "job_changed", "检测任务已变化，请刷新页面"))?;
    json_reply(&JobReply { job: Some(job) })
}

fn job_active() -> ApiError {
    ApiError::new(409, "job_active", "已有检测在进行，请等待完成或先停止")
}

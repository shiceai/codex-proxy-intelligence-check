//! 后台检测任务：页面提交的一批"账号 × 题目"，由维护阶段的后台处理逐题执行。

use crate::{
    host_calls,
    results::{Effort, Verdict},
};
use gateway_plugin_sdk::{ErrorCode, PluginFault, client::HostClient};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const NAMESPACE: &str = "jobs";
const KEY: &str = "current";
const SCHEMA_VERSION: u32 = 1;
const WRITE_ATTEMPTS: usize = 5;

/// 单批任务上限：任务记录要放进单条 256 KiB 的私有状态。
pub const MAX_TASKS: usize = 1000;
pub const STOPPED: &str = "已停止";
pub const INTERRUPTED: &str = "检测被中断：插件停用、升级或宿主重启";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    /// 已请求停止：不再开始新题，进行中的题完成后结束。
    Stopping,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Running,
    Done,
    Skipped,
    Interrupted,
}

/// 结果摘要，供页面更新色块条；完整结果在 results 命名空间。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResult {
    pub id: String,
    pub verdict: Verdict,
    pub created_at_ms: u64,
    pub saved: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub account_id: String,
    pub question_id: String,
    pub state: TaskState,
    pub result: Option<TaskResult>,
    /// 未执行原因或中断说明。
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: String,
    pub created_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub key_id: String,
    pub key_name: String,
    pub model: String,
    pub effort: Option<Effort>,
    pub status: JobStatus,
    pub tasks: Vec<Task>,
}

/// 一批任务共用的执行参数。
pub struct NewJob {
    pub key_id: String,
    pub key_name: String,
    pub model: String,
    pub effort: Option<Effort>,
}

/// 按账号优先展开"账号 × 题目"；重复的账号只保留第一次。
///
/// # 错误
///
/// 没有账号、没有题目或超过单批上限时返回说明。
pub fn build(
    now_ms: u64,
    new: NewJob,
    account_ids: &[String],
    question_ids: &[String],
) -> Result<Job, String> {
    let mut seen = BTreeSet::new();
    let accounts: Vec<&String> = account_ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .collect();
    if accounts.is_empty() {
        return Err("请至少选择一个账号".to_owned());
    }
    if question_ids.is_empty() {
        return Err("题库中没有启用的题目".to_owned());
    }
    if accounts.len().saturating_mul(question_ids.len()) > MAX_TASKS {
        return Err("单批最多 1000 道题（账号数 × 题目数）".to_owned());
    }
    let tasks = accounts
        .iter()
        .flat_map(|account_id| {
            question_ids.iter().map(|question_id| Task {
                account_id: (*account_id).clone(),
                question_id: question_id.clone(),
                state: TaskState::Pending,
                result: None,
                note: None,
            })
        })
        .collect();
    Ok(Job {
        id: format!("j{now_ms}"),
        created_at_ms: now_ms,
        finished_at_ms: None,
        key_id: new.key_id,
        key_name: new.key_name,
        model: new.model,
        effort: new.effort,
        status: JobStatus::Running,
        tasks,
    })
}

#[must_use]
pub fn is_active(job: &Job) -> bool {
    job.status != JobStatus::Finished
}

/// 第一个待执行、且账号当前没有进行中检测的任务。
#[must_use]
pub fn next_runnable(job: &Job, busy: &BTreeSet<String>) -> Option<usize> {
    job.tasks
        .iter()
        .position(|task| task.state == TaskState::Pending && !busy.contains(&task.account_id))
}

/// 上一轮遗留的进行中任务已被宿主取消；请求可能已计费，标为中断而不重跑。
pub fn interrupt_running(job: &mut Job) {
    for task in &mut job.tasks {
        if task.state == TaskState::Running {
            task.state = TaskState::Interrupted;
            task.note = Some(INTERRUPTED.to_owned());
        }
    }
}

/// 停止时跳过剩余任务；没有待执行和进行中的任务时结束本批。
pub fn settle(job: &mut Job, now_ms: u64) {
    if job.status == JobStatus::Stopping {
        for task in &mut job.tasks {
            if task.state == TaskState::Pending {
                task.state = TaskState::Skipped;
                task.note = Some(STOPPED.to_owned());
            }
        }
    }
    let idle = job
        .tasks
        .iter()
        .all(|task| !matches!(task.state, TaskState::Pending | TaskState::Running));
    if idle && is_active(job) {
        job.status = JobStatus::Finished;
        job.finished_at_ms = Some(now_ms);
    }
}

/// 读取当前任务及其版本号。
///
/// # 错误
///
/// 宿主回调失败或记录无法解析时返回错误。
pub async fn load(host: &HostClient) -> Result<Option<(u64, Job)>, PluginFault> {
    let Some(record) = host_calls::get_state(host, NAMESPACE, KEY).await? else {
        return Ok(None);
    };
    if record.schema_version != SCHEMA_VERSION {
        return Err(invalid_state());
    }
    let job = serde_json::from_value(record.value).map_err(|_| invalid_state())?;
    Ok(Some((record.version, job)))
}

/// 写入新任务；`expected_version` 是读取时看到的版本，期间有人写入则返回冲突。
///
/// # 错误
///
/// 版本冲突或宿主回调失败时返回错误。
pub async fn create(
    host: &HostClient,
    job: &Job,
    expected_version: Option<u64>,
) -> Result<(), PluginFault> {
    let value = serde_json::to_value(job).map_err(|_| invalid_state())?;
    host_calls::put_state(host, NAMESPACE, KEY, value, expected_version)
        .await
        .map(|_| ())
}

/// 读取 → 修改 → 按版本写回；冲突时重新读取并重放同一修改。`change` 返回 false 表示无需写入。
///
/// # 错误
///
/// 宿主回调失败或连续冲突时返回错误。
pub async fn update(
    host: &HostClient,
    mut change: impl FnMut(&mut Job) -> bool,
) -> Result<Option<Job>, PluginFault> {
    let mut attempt = 1;
    loop {
        let Some((version, mut job)) = load(host).await? else {
            return Ok(None);
        };
        if !change(&mut job) {
            return Ok(Some(job));
        }
        match create(host, &job, Some(version)).await {
            Ok(()) => return Ok(Some(job)),
            Err(fault) if fault.code == ErrorCode::Conflict && attempt < WRITE_ATTEMPTS => {
                attempt += 1;
            }
            Err(fault) => return Err(fault),
        }
    }
}

fn invalid_state() -> PluginFault {
    PluginFault::new(ErrorCode::Fault, "已保存的检测任务无效")
}

#[cfg(test)]
mod tests {
    use super::{
        INTERRUPTED, Job, JobStatus, MAX_TASKS, NewJob, STOPPED, TaskState, build,
        interrupt_running, is_active, next_runnable, settle,
    };
    use std::collections::BTreeSet;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn new_job() -> NewJob {
        NewJob {
            key_id: "key_1".to_owned(),
            key_name: "主 Key".to_owned(),
            model: "gpt-6-astra".to_owned(),
            effort: None,
        }
    }

    fn job(accounts: &[&str], questions: &[&str]) -> Job {
        build(1000, new_job(), &ids(accounts), &ids(questions)).unwrap()
    }

    #[test]
    fn build_expands_accounts_first() {
        let job = job(&["a", "b"], &["q1", "q2"]);
        let order: Vec<_> = job
            .tasks
            .iter()
            .map(|task| format!("{}:{}", task.account_id, task.question_id))
            .collect();
        assert_eq!(order, ["a:q1", "a:q2", "b:q1", "b:q2"]);
        assert_eq!(job.id, "j1000");
        assert_eq!(job.status, JobStatus::Running);
        assert!(
            job.tasks
                .iter()
                .all(|task| task.state == TaskState::Pending)
        );
    }

    #[test]
    fn build_dedupes_accounts() {
        assert_eq!(job(&["a", "a", "b"], &["q1"]).tasks.len(), 2);
    }

    #[test]
    fn build_rejects_invalid_batches() {
        let empty: [&str; 0] = [];
        assert_eq!(
            build(1, new_job(), &ids(&empty), &ids(&["q"])).unwrap_err(),
            "请至少选择一个账号"
        );
        assert_eq!(
            build(1, new_job(), &ids(&["a"]), &ids(&empty)).unwrap_err(),
            "题库中没有启用的题目"
        );
        let accounts: Vec<String> = (0..=MAX_TASKS).map(|index| format!("a{index}")).collect();
        assert_eq!(
            build(1, new_job(), &accounts, &ids(&["q"])).unwrap_err(),
            "单批最多 1000 道题（账号数 × 题目数）"
        );
    }

    #[test]
    fn next_runnable_skips_busy_accounts_and_started_tasks() {
        let mut job = job(&["a", "b"], &["q1", "q2"]);
        job.tasks[0].state = TaskState::Done;
        let busy = BTreeSet::from(["a".to_owned()]);
        assert_eq!(next_runnable(&job, &busy), Some(2));
        assert_eq!(next_runnable(&job, &BTreeSet::new()), Some(1));
    }

    #[test]
    fn interrupt_running_marks_leftovers() {
        let mut job = job(&["a", "b"], &["q1"]);
        job.tasks[0].state = TaskState::Running;
        interrupt_running(&mut job);
        assert_eq!(job.tasks[0].state, TaskState::Interrupted);
        assert_eq!(job.tasks[0].note.as_deref(), Some(INTERRUPTED));
        assert_eq!(job.tasks[1].state, TaskState::Pending);
    }

    #[test]
    fn settle_stopping_skips_pending_and_waits_for_running() {
        let mut job = job(&["a", "b", "c"], &["q1"]);
        job.status = JobStatus::Stopping;
        job.tasks[0].state = TaskState::Running;
        settle(&mut job, 2000);
        assert_eq!(job.tasks[1].state, TaskState::Skipped);
        assert_eq!(job.tasks[1].note.as_deref(), Some(STOPPED));
        assert_eq!(job.status, JobStatus::Stopping);
        job.tasks[0].state = TaskState::Done;
        settle(&mut job, 3000);
        assert_eq!(job.status, JobStatus::Finished);
        assert_eq!(job.finished_at_ms, Some(3000));
        assert!(!is_active(&job));
    }

    #[test]
    fn settle_finishes_only_when_nothing_left() {
        let mut job = job(&["a"], &["q1", "q2"]);
        job.tasks[0].state = TaskState::Done;
        settle(&mut job, 2000);
        assert_eq!(job.status, JobStatus::Running);
        assert!(is_active(&job));
        job.tasks[1].state = TaskState::Skipped;
        settle(&mut job, 2000);
        assert_eq!(job.status, JobStatus::Finished);
    }
}

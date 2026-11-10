//! 后台执行：宿主周期调用 `plugin.reconcile`，逐题执行页面提交的检测任务。
//!
//! 题目在处理函数自身的 future 内并发推进，不另起 tokio 任务：宿主只保证驱动父调用。

use crate::{
    check::{self, CHECK_TIMEOUT, Target},
    host_calls,
    jobs::{self, Job, JobStatus, TaskResult, TaskState},
    now_ms, questions,
    results::{self, excerpt},
};
use gateway_plugin_sdk::{PluginFault, call::host::ClientKey, client::HostClient};
use std::{
    collections::BTreeSet,
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};

const CONCURRENCY: usize = 3;
/// 剩余预算不足时本轮不再开始新题，留给下一轮继续。
const START_MARGIN: Duration = Duration::from_secs(CHECK_TIMEOUT.as_secs() + 20);
const ACCOUNT_UNAVAILABLE: &str = "账号不存在、已停用或不是 OpenAI 账号";
const QUESTION_UNAVAILABLE: &str = "题目不存在或已停用";
const UNSAVED: &str = "结果写入失败，未保存";

enum Outcome {
    Done(TaskResult),
    Skipped(String),
}

type Running = Vec<Pin<Box<dyn Future<Output = (usize, Outcome)> + Send>>>;

pub(crate) fn can_start(remaining: Option<Duration>) -> bool {
    remaining.is_none_or(|remaining| remaining >= START_MARGIN)
}

/// 宿主错误说明可能很长，截断后才能放进任务记录（schema 限 300 字）。
pub(crate) fn host_failure(message: &str) -> String {
    excerpt(&format!("宿主操作失败：{message}"))
}

/// 一轮后台处理：先把上一轮遗留的进行中题目标为中断，再在预算内逐题执行。
///
/// # 错误
///
/// 读写任务记录失败时返回错误，宿主 5 秒后重试。
pub(crate) async fn reconcile(host: &HostClient) -> Result<(), PluginFault> {
    let Some(mut job) = jobs::update(host, |job| {
        let before = job.clone();
        jobs::interrupt_running(job);
        jobs::settle(job, now_ms());
        *job != before
    })
    .await?
    else {
        return Ok(());
    };
    let job_id = job.id.clone();
    let mut running: Running = Vec::new();
    let mut busy = BTreeSet::new();
    // 写任务记录出错后不再开始新题，但已发出的题必须跑完：请求已计费，结果不能丢。
    let mut halted = false;
    let mut start_failure = None;
    let mut unrecorded = Vec::new();
    loop {
        while !halted
            && running.len() < CONCURRENCY
            && job.status == JobStatus::Running
            && can_start(host.remaining_budget())
        {
            let Some(index) = jobs::next_runnable(&job, &busy) else {
                break;
            };
            // 先记为进行中再发请求：本轮被取消时，下一轮据此识别被中断的题。
            match jobs::update(host, |job| start(job, &job_id, index)).await {
                Ok(Some(updated)) if updated.id == job_id => job = updated,
                // 记录被删除或换成另一批（正常不会发生：有进行中的任务时不允许提交）。
                Ok(_) => {
                    halted = true;
                    break;
                }
                Err(fault) => {
                    halted = true;
                    start_failure = Some(fault);
                    break;
                }
            }
            if job.tasks[index].state != TaskState::Running {
                continue;
            }
            busy.insert(job.tasks[index].account_id.clone());
            running.push(Box::pin(run_task(host.clone(), job.clone(), index)));
        }
        let Some((index, outcome)) = next_finished(&mut running).await else {
            break;
        };
        busy.remove(&job.tasks[index].account_id);
        match jobs::update(host, |job| finish(job, &job_id, index, &outcome)).await {
            Ok(Some(updated)) => job = updated,
            Ok(None) => {}
            Err(_) => {
                halted = true;
                unrecorded.push((index, outcome));
            }
        }
    }
    // 补记写入失败的结果，并在没有进行中的题时结束本批。
    jobs::update(host, |job| {
        let before = job.clone();
        for (index, outcome) in &unrecorded {
            finish(job, &job_id, *index, outcome);
        }
        jobs::settle(job, now_ms());
        *job != before
    })
    .await?;
    start_failure.map_or(Ok(()), Err)
}

fn start(job: &mut Job, job_id: &str, index: usize) -> bool {
    let startable = job.id == job_id
        && job.status == JobStatus::Running
        && job
            .tasks
            .get(index)
            .is_some_and(|task| task.state == TaskState::Pending);
    if startable {
        job.tasks[index].state = TaskState::Running;
    }
    startable
}

fn finish(job: &mut Job, job_id: &str, index: usize, outcome: &Outcome) -> bool {
    if job.id != job_id {
        return false;
    }
    let Some(task) = job.tasks.get_mut(index) else {
        return false;
    };
    match outcome {
        Outcome::Done(result) => {
            task.state = TaskState::Done;
            task.note = (!result.saved).then(|| UNSAVED.to_owned());
            task.result = Some(result.clone());
        }
        Outcome::Skipped(reason) => {
            task.state = TaskState::Skipped;
            task.note = Some(reason.clone());
        }
    }
    jobs::settle(job, now_ms());
    true
}

/// 依次轮询进行中的题目，返回最先完成的一道。
async fn next_finished(running: &mut Running) -> Option<(usize, Outcome)> {
    if running.is_empty() {
        return None;
    }
    let finished = poll_fn(|context| {
        for slot in 0..running.len() {
            if let Poll::Ready(output) = running[slot].as_mut().poll(context) {
                drop(running.swap_remove(slot));
                return Poll::Ready(output);
            }
        }
        Poll::Pending
    })
    .await;
    Some(finished)
}

/// 执行一道题：执行前再次核对题目与账号，结果写入该账号的记录。
async fn run_task(host: HostClient, job: Job, index: usize) -> (usize, Outcome) {
    let task = &job.tasks[index];
    let question = match questions::load(&host).await {
        Ok(bank) => bank
            .items
            .into_iter()
            .find(|question| question.id == task.question_id && question.enabled),
        Err(fault) => {
            return (index, Outcome::Skipped(host_failure(&fault.message)));
        }
    };
    let Some(question) = question else {
        return (index, Outcome::Skipped(QUESTION_UNAVAILABLE.to_owned()));
    };
    let account = host_calls::runtime_account(&host, &task.account_id)
        .await
        .ok()
        .filter(|account| account.enabled && account.provider_id == "openai");
    let Some(account) = account else {
        return (index, Outcome::Skipped(ACCOUNT_UNAVAILABLE.to_owned()));
    };
    let key = ClientKey {
        id: job.key_id.clone(),
        name: job.key_name.clone(),
        enabled: true,
    };
    let result = check::run(
        &host,
        Target {
            account_label: host_calls::account_label(&account),
            account: &account,
            key: &key,
            model: &job.model,
            effort: job.effort,
            question: &question,
        },
    )
    .await;
    let summary = TaskResult {
        id: result.id.clone(),
        verdict: result.verdict,
        created_at_ms: result.created_at_ms,
        saved: false,
    };
    let saved = results::append(&host, result).await.is_ok();
    (index, Outcome::Done(TaskResult { saved, ..summary }))
}

#[cfg(test)]
mod tests {
    use super::{can_start, host_failure};
    use std::time::Duration;

    #[test]
    fn host_failure_note_fits_the_schema() {
        let note = host_failure(&"x".repeat(1000));
        assert!(note.starts_with("宿主操作失败："));
        assert_eq!(note.chars().count(), 300);
    }

    #[test]
    fn stops_starting_when_budget_is_low() {
        assert!(can_start(None));
        assert!(can_start(Some(Duration::from_secs(170))));
        assert!(!can_start(Some(Duration::from_secs(169))));
    }
}

//! 检测结果：每个账号一条私有状态记录，最新的在前，最多保留 20 次。

use crate::{host_calls, judge::MatchMode};
use gateway_plugin_sdk::{ErrorCode, PluginFault, client::HostClient};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};

const NAMESPACE: &str = "results";
const SCHEMA_VERSION: u32 = 1;
const WRITE_ATTEMPTS: usize = 3;
/// 每个账号保留的结果条数。
pub const KEEP: usize = 20;
/// 答案摘录与错误说明的最大字符数。
pub const EXCERPT_CHARS: usize = 300;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// 请求成功完成，答案比对通过。
    Normal,
    /// 请求成功完成，答案比对失败。
    Failed,
    /// 宿主拒绝、上游报错、超时或没有最终答案；不计入通过率。
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
}

/// 一次检测的结果；题目信息以快照写入，之后修改题库不影响历史。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    pub id: String,
    pub created_at_ms: u64,
    pub account_id: String,
    pub account_label: String,
    pub key_id: String,
    pub key_name: String,
    pub model: String,
    pub response_model: Option<String>,
    pub effort: Option<Effort>,
    pub question_id: String,
    pub question_name: String,
    pub expected: String,
    #[serde(rename = "match")]
    pub match_mode: MatchMode,
    pub verdict: Verdict,
    pub answer_excerpt: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub request_id: Option<String>,
}

/// 页面画色块条和算通过率所需的摘要。
#[derive(Debug, PartialEq, Serialize)]
pub struct Recent {
    pub id: String,
    pub verdict: Verdict,
    pub created_at_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    items: Vec<CheckResult>,
}

/// 按字符截取，避免在 UTF-8 字符中间截断。
#[must_use]
pub fn excerpt(text: &str) -> String {
    text.chars().take(EXCERPT_CHARS).collect()
}

/// 进程内唯一的结果 ID。
#[must_use]
pub fn new_id(now_ms: u64) -> String {
    format!("r{now_ms}_{}", SEQUENCE.fetch_add(1, Ordering::Relaxed))
}

#[must_use]
pub fn prepend(mut items: Vec<CheckResult>, result: CheckResult) -> Vec<CheckResult> {
    items.insert(0, result);
    items.truncate(KEEP);
    items
}

#[must_use]
pub fn recent(items: &[CheckResult]) -> Vec<Recent> {
    items
        .iter()
        .map(|item| Recent {
            id: item.id.clone(),
            verdict: item.verdict,
            created_at_ms: item.created_at_ms,
        })
        .collect()
}

/// 读取账号的结果，最新的在前；没有记录时为空。
///
/// # 错误
///
/// 宿主回调失败或已保存的记录无法解析时返回错误。
pub async fn list(host: &HostClient, account_id: &str) -> Result<Vec<CheckResult>, PluginFault> {
    Ok(read(host, account_id).await?.1)
}

/// 把结果追加到账号记录；版本冲突时重新读取，最多尝试 3 次。
///
/// # 错误
///
/// 宿主回调失败（含超出配额）或连续冲突时返回错误。
pub async fn append(host: &HostClient, result: CheckResult) -> Result<(), PluginFault> {
    let mut attempt = 1;
    loop {
        let (version, items) = read(host, &result.account_id).await?;
        let value = json!({ "items": prepend(items, result.clone()) });
        match host_calls::put_state(host, NAMESPACE, &result.account_id, value, version).await {
            Err(fault) if fault.code == ErrorCode::Conflict && attempt < WRITE_ATTEMPTS => {
                attempt += 1;
            }
            other => return other.map(|_| ()),
        }
    }
}

async fn read(
    host: &HostClient,
    account_id: &str,
) -> Result<(Option<u64>, Vec<CheckResult>), PluginFault> {
    let Some(record) = host_calls::get_state(host, NAMESPACE, account_id).await? else {
        return Ok((None, Vec::new()));
    };
    if record.schema_version != SCHEMA_VERSION {
        return Err(invalid_state());
    }
    let stored: Stored = serde_json::from_value(record.value).map_err(|_| invalid_state())?;
    Ok((Some(record.version), stored.items))
}

fn invalid_state() -> PluginFault {
    PluginFault::new(ErrorCode::Fault, "已保存的检测结果无效")
}

#[cfg(test)]
mod tests {
    use super::{CheckResult, Effort, KEEP, Verdict, excerpt, new_id, prepend, recent};
    use crate::judge::MatchMode;
    use serde_json::json;

    fn sample(id: &str, created_at_ms: u64) -> CheckResult {
        CheckResult {
            id: id.to_owned(),
            created_at_ms,
            account_id: "acct_1".to_owned(),
            account_label: "a@x.com".to_owned(),
            key_id: "key_1".to_owned(),
            key_name: "测试 Key".to_owned(),
            model: "gpt-5.5".to_owned(),
            response_model: Some("gpt-5.5".to_owned()),
            effort: Some(Effort::High),
            question_id: "candy".to_owned(),
            question_name: "糖果配对".to_owned(),
            expected: "21".to_owned(),
            match_mode: MatchMode::Exact,
            verdict: Verdict::Normal,
            answer_excerpt: "21".to_owned(),
            error: None,
            duration_ms: 1200,
            input_tokens: Some(10),
            output_tokens: Some(20),
            reasoning_tokens: Some(15),
            request_id: Some("req_1".to_owned()),
        }
    }

    #[test]
    fn excerpt_truncates_by_chars() {
        let long = "糖".repeat(400);
        assert_eq!(excerpt(&long).chars().count(), 300);
        assert_eq!(excerpt("21"), "21");
    }

    #[test]
    fn prepend_keeps_newest_twenty() {
        let old: Vec<_> = (0..20).map(|i| sample(&format!("r{i}"), i)).collect();
        let items = prepend(old, sample("new", 99));
        assert_eq!(items.len(), KEEP);
        assert_eq!(items[0].id, "new");
        assert_eq!(items[1].id, "r0");
        assert!(items.iter().all(|item| item.id != "r19"));
    }

    #[test]
    fn recent_maps_fields() {
        let summary = serde_json::to_value(recent(&[sample("r1", 5)])).unwrap();
        assert_eq!(
            summary,
            json!([{ "id": "r1", "verdict": "normal", "created_at_ms": 5 }])
        );
    }

    #[test]
    fn verdict_and_effort_serialize() {
        assert_eq!(serde_json::to_value(Verdict::Failed).unwrap(), "failed");
        assert_eq!(serde_json::to_value(Effort::Xhigh).unwrap(), "xhigh");
        let value = serde_json::to_value(sample("r1", 1)).unwrap();
        assert_eq!(value["match"], "exact");
        assert_eq!(value["effort"], "high");
    }

    #[test]
    fn new_ids_are_unique() {
        assert_ne!(new_id(1), new_id(1));
        assert!(new_id(7).starts_with("r7_"));
    }
}

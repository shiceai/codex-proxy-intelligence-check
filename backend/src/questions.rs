//! 题库：内置糖果题写在代码里，自定义题与启用状态存入插件私有状态。

use crate::{host_calls, judge::MatchMode};
use gateway_plugin_sdk::{PluginFault, call::host::StateRecord, client::HostClient};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

const NAMESPACE: &str = "questions";
const KEY: &str = "bank";
const SCHEMA_VERSION: u32 = 1;
const MAX_QUESTIONS: usize = 20;
pub(crate) const CANDY_ID: &str = "candy";
/// 社区流传的糖果题，标准答案为 21。
const CANDY_PROMPT: &str = "不允许调用工具和联网。在一个黑色的袋子里放有三种口味的糖果，每种糖果有两种不同的形状，分别为圆形和五角星形，不同的形状靠手感可以分辨，现已知苹果味圆形有7颗、苹果味五角星形有7颗、桃子味圆形有9颗、桃子味五角星形有6颗、西瓜味圆形有8颗、西瓜味五角星形有4颗，参赛者需要在活动前决定摸出的糖果数目，那么最少取出多少个糖果才能保证手中同时拥有不同形状的苹果味和桃子味的糖，同时手中有圆形苹果味匹配五角星桃子味糖果，或者有圆形桃子味匹配五角星苹果味糖果都满足要求，最终只回复数字结果。";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Question {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub expected: String,
    #[serde(rename = "match")]
    pub match_mode: MatchMode,
    pub enabled: bool,
    pub builtin: bool,
}

#[derive(Serialize)]
pub(crate) struct Bank {
    pub version: Option<u64>,
    pub items: Vec<Question>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    items: Vec<Question>,
}

pub(crate) fn candy() -> Question {
    Question {
        id: CANDY_ID.to_owned(),
        name: "糖果配对".to_owned(),
        prompt: CANDY_PROMPT.to_owned(),
        expected: "21".to_owned(),
        match_mode: MatchMode::Exact,
        enabled: true,
        builtin: true,
    }
}

fn builtins() -> [Question; 1] {
    [candy()]
}

/// 解析保存的题库；没有记录时只有内置糖果题。
pub(crate) fn from_stored(record: Option<StateRecord>) -> Result<Bank, String> {
    let Some(record) = record else {
        return Ok(Bank {
            version: None,
            items: builtins().to_vec(),
        });
    };
    if record.schema_version != SCHEMA_VERSION {
        return Err("已保存的题库格式不受支持".to_owned());
    }
    let stored: Stored =
        serde_json::from_value(record.value).map_err(|_| "已保存的题库无效".to_owned())?;
    Ok(Bank {
        version: Some(record.version),
        items: with_builtins(stored.items),
    })
}

pub(crate) async fn load(host: &HostClient) -> Result<Bank, PluginFault> {
    let record = host_calls::get_state(host, NAMESPACE, KEY).await?;
    from_stored(record)
        .map_err(|message| PluginFault::new(gateway_plugin_sdk::ErrorCode::Fault, message))
}

/// 按版本写入合并后的题库，返回新版本号。
pub(crate) async fn save(
    host: &HostClient,
    items: &[Question],
    expected_version: Option<u64>,
) -> Result<u64, PluginFault> {
    host_calls::put_state(
        host,
        NAMESPACE,
        KEY,
        json!({ "items": items }),
        expected_version,
    )
    .await
}

/// 把页面提交的整张题库与当前题库合并：内置题只接受启停，新题分配 ID。
pub(crate) fn merge(
    current: &[Question],
    submitted: Vec<Question>,
    now_ms: u64,
) -> Result<Vec<Question>, String> {
    if submitted.len() > MAX_QUESTIONS {
        return Err("题库最多 20 道题".to_owned());
    }
    if builtins().iter().any(|builtin| {
        !submitted
            .iter()
            .any(|item| item.builtin && item.id == builtin.id)
    }) {
        return Err("内置题不能删除".to_owned());
    }
    let mut seen = BTreeSet::new();
    let mut merged = Vec::with_capacity(submitted.len());
    for (index, question) in submitted.into_iter().enumerate() {
        let item = if question.builtin {
            let builtin = builtins()
                .into_iter()
                .find(|builtin| builtin.id == question.id)
                .ok_or_else(|| format!("题目不存在：{}", question.id))?;
            Question {
                enabled: question.enabled,
                ..builtin
            }
        } else {
            let id = if question.id.is_empty() {
                format!("q_{now_ms}_{index}")
            } else if current
                .iter()
                .any(|item| !item.builtin && item.id == question.id)
            {
                question.id.clone()
            } else {
                return Err(format!("题目不存在：{}", question.id));
            };
            validated(Question { id, ..question })?
        };
        if !seen.insert(item.id.clone()) {
            return Err("题目 ID 重复".to_owned());
        }
        merged.push(item);
    }
    Ok(with_builtins(merged))
}

/// 内置题内容一律取代码常量，只沿用保存的启用状态；缺失的内置题补在最前面。
fn with_builtins(items: Vec<Question>) -> Vec<Question> {
    let mut result: Vec<Question> = builtins()
        .into_iter()
        .map(|builtin| {
            let enabled = items
                .iter()
                .find(|item| item.builtin && item.id == builtin.id)
                .is_none_or(|item| item.enabled);
            Question { enabled, ..builtin }
        })
        .collect();
    result.extend(items.into_iter().filter(|item| !item.builtin));
    result
}

fn validated(question: Question) -> Result<Question, String> {
    let name = question.name.trim();
    let prompt = question.prompt.trim();
    let expected = question.expected.trim();
    if !bounded(name, 40, false) {
        return Err("题目名称需为 1–40 个字符".to_owned());
    }
    if !bounded(prompt, 2000, true) {
        return Err("题面需为 1–2000 个字符".to_owned());
    }
    if !bounded(expected, 200, false) {
        return Err("标准答案需为 1–200 个字符".to_owned());
    }
    Ok(Question {
        name: name.to_owned(),
        prompt: prompt.to_owned(),
        expected: expected.to_owned(),
        builtin: false,
        ..question
    })
}

/// 按字符计数；题面保留换行与缩进，其余字段不允许控制字符。
fn bounded(text: &str, maximum: usize, multiline: bool) -> bool {
    (1..=maximum).contains(&text.chars().count())
        && !text
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
}

#[cfg(test)]
mod tests {
    use super::{CANDY_ID, MatchMode, Question, candy, from_stored, merge};
    use gateway_plugin_sdk::call::host::StateRecord;
    use serde_json::json;

    fn custom(id: &str, name: &str) -> Question {
        Question {
            id: id.to_owned(),
            name: name.to_owned(),
            prompt: "1+1=?".to_owned(),
            expected: "2".to_owned(),
            match_mode: MatchMode::Exact,
            enabled: true,
            builtin: false,
        }
    }

    fn tampered_candy(enabled: bool) -> Question {
        Question {
            prompt: "x".to_owned(),
            enabled,
            ..candy()
        }
    }

    #[test]
    fn absent_record_yields_candy() {
        let bank = from_stored(None).unwrap();
        assert_eq!(bank.version, None);
        assert_eq!(bank.items, vec![candy()]);
        assert_eq!(bank.items[0].id, CANDY_ID);
        assert!(bank.items[0].prompt.starts_with("不允许调用工具和联网。"));
        assert!(bank.items[0].prompt.ends_with("最终只回复数字结果。"));
    }

    #[test]
    fn stored_builtin_content_is_overridden() {
        let record = StateRecord {
            value: json!({ "items": [tampered_candy(false), custom("q_1_0", "加法")] }),
            version: 7,
            schema_version: 1,
        };
        let bank = from_stored(Some(record)).unwrap();
        assert_eq!(bank.version, Some(7));
        assert_eq!(bank.items[0].prompt, candy().prompt);
        assert!(!bank.items[0].enabled);
        assert_eq!(bank.items[1].id, "q_1_0");
    }

    #[test]
    fn stored_bank_without_builtin_gets_candy_first() {
        let record = StateRecord {
            value: json!({ "items": [custom("q_1_0", "加法")] }),
            version: 2,
            schema_version: 1,
        };
        let bank = from_stored(Some(record)).unwrap();
        assert_eq!(bank.items[0], candy());
        assert_eq!(bank.items.len(), 2);
    }

    #[test]
    fn unsupported_schema_is_rejected() {
        let record = StateRecord {
            value: json!({ "items": [] }),
            version: 2,
            schema_version: 2,
        };
        assert_eq!(
            from_stored(Some(record)).err().unwrap(),
            "已保存的题库格式不受支持"
        );
    }

    #[test]
    fn merge_assigns_ids_and_keeps_builtin_content() {
        let items = merge(
            &[candy()],
            vec![tampered_candy(false), custom("", "加法")],
            1000,
        )
        .unwrap();
        assert_eq!(items[0].prompt, candy().prompt);
        assert!(!items[0].enabled);
        assert_eq!(items[1].id, "q_1000_1");
        assert_eq!(items[1].name, "加法");
    }

    #[test]
    fn merge_rejects_invalid_banks() {
        let current = [candy(), custom("q_1_1", "加法")];
        let cases: Vec<(Vec<Question>, &str)> = vec![
            (vec![custom("", "加法")], "内置题不能删除"),
            (vec![candy(), custom("q_9_9", "加法")], "题目不存在：q_9_9"),
            (
                vec![candy(), custom("q_1_1", "加法"), custom("q_1_1", "加法")],
                "题目 ID 重复",
            ),
            (
                std::iter::once(candy())
                    .chain((0..20).map(|_| custom("", "加法")))
                    .collect(),
                "题库最多 20 道题",
            ),
            (
                vec![candy(), custom("", &"糖".repeat(41))],
                "题目名称需为 1–40 个字符",
            ),
            (
                vec![
                    candy(),
                    Question {
                        prompt: "a".repeat(2001),
                        ..custom("", "加法")
                    },
                ],
                "题面需为 1–2000 个字符",
            ),
            (
                vec![
                    candy(),
                    Question {
                        expected: " ".to_owned(),
                        ..custom("", "加法")
                    },
                ],
                "标准答案需为 1–200 个字符",
            ),
        ];
        for (submitted, message) in cases {
            assert_eq!(merge(&current, submitted, 1).unwrap_err(), message);
        }
    }
}

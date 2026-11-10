//! 答案归一化与判分：只比较模型最终可见的答案文本。

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

const FENCE: &str = "```";

/// 标准答案的匹配方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    /// 归一化后完全相等。
    Exact,
    /// 归一化后包含标准答案。
    Contains,
}

/// NFKC 归一化并去掉首尾空白；整段被一层 Markdown 代码块包裹时先剥掉这一层。
#[must_use]
pub fn normalize(text: &str) -> String {
    let text: String = text.nfkc().collect();
    let text = text.trim();
    strip_fence(text).unwrap_or(text).trim().to_owned()
}

/// 答案与标准答案都归一化后按匹配方式比较。
#[must_use]
pub fn passes(answer: &str, expected: &str, mode: MatchMode) -> bool {
    let answer = normalize(answer);
    let expected = normalize(expected);
    match mode {
        MatchMode::Exact => answer == expected,
        MatchMode::Contains => answer.contains(&expected),
    }
}

fn strip_fence(text: &str) -> Option<&str> {
    let inner = text.strip_prefix(FENCE)?.strip_suffix(FENCE)?;
    // 多行时开头一行是可选的语言标记，如 ```text；单行 ```21``` 整段都是答案。
    Some(inner.split_once('\n').map_or(inner, |(_, body)| body))
}

#[cfg(test)]
mod tests {
    use super::{MatchMode, passes};

    #[test]
    fn exact_mode_matches_spec_examples() {
        for (answer, expected) in [
            ("21", true),
            ("  21\n", true),
            ("２１", true),
            ("```\n21\n```", true),
            ("```text\n21\n```", true),
            ("```21```", true),
            ("答案是21", false),
            ("21。", false),
            ("21 or 29", false),
            ("29", false),
        ] {
            assert_eq!(
                passes(answer, "21", MatchMode::Exact),
                expected,
                "{answer:?}"
            );
        }
    }

    #[test]
    fn contains_mode_matches_substring() {
        assert!(passes("答案是21", "21", MatchMode::Contains));
        assert!(passes("２１颗", "21", MatchMode::Contains));
        assert!(!passes("29", "21", MatchMode::Contains));
    }
}

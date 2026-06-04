//! 把命中改写成「给 LLM 的修改指令」。
//!
//! 每一项 [`Suggestion`] 是一次「在第 X 行，把 `phrase` 改写成更具体的描写」
//! 的人话指令。LLM 拿到清单后可以一次性把全章过一遍去 AI 化。

use serde::{Deserialize, Serialize};

use super::dictionary::ClicheHit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// 触发套话。
    pub phrase: String,
    /// 出现次数。
    pub count: u32,
    /// 第一次出现的行号。
    pub first_line: Option<u32>,
    /// 严重度（1-3）。
    pub severity: u8,
    /// 给 LLM 看的修改建议（中文）。
    pub instruction: String,
    /// 套话本身的原因（短）。
    pub reason: String,
}

/// 按行号排序，便于 LLM 按顺序处理。
pub fn build_suggestions(hits: &[ClicheHit]) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = hits
        .iter()
        .map(|h| Suggestion {
            phrase: h.entry.phrase.clone(),
            count: h.count,
            first_line: h.first_line,
            severity: h.entry.severity,
            instruction: h.entry.suggestion.clone(),
            reason: h.entry.reason.clone(),
        })
        .collect();
    out.sort_by(|a, b| {
        a.first_line
            .unwrap_or(u32::MAX)
            .cmp(&b.first_line.unwrap_or(u32::MAX))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deai::dictionary::{ClicheCategory, ClicheDictionary, ClicheEntry};

    fn make_hit(phrase: &str, line: u32) -> ClicheHit {
        ClicheHit {
            entry: ClicheEntry {
                phrase: phrase.into(),
                category: ClicheCategory::AbstractDescription,
                severity: 2,
                reason: "测试".into(),
                suggestion: "改".into(),
            },
            count: 1,
            first_line: Some(line),
        }
    }

    #[test]
    fn suggestions_sorted_by_line() {
        let dict = ClicheDictionary::builtin();
        let hits = vec![make_hit("仿佛", 5), make_hit("似乎", 2), make_hit("然而", 10)];
        let s = build_suggestions(&hits);
        assert_eq!(s[0].first_line, Some(2));
        assert_eq!(s[1].first_line, Some(5));
        assert_eq!(s[2].first_line, Some(10));
        // 防止 dict 变更把测试搞挂：仅断言排序顺序
        let _ = dict;
    }
}

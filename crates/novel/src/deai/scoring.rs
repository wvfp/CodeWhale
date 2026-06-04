//! AI 味打分。
//!
//! 给定一段正文 + 词典，输出 0-100 的「人类度」分数（越高越像人写的）
//! 和命中清单。打分公式：
//!
//! - 基础分 100
//! - 严重度 1：每次 -1 分
//! - 严重度 2：每次 -2 分
//! - 严重度 3：每次 -4 分
//! - 文本长度因子：长度越长，对扣分越宽容（因为长文里出现几次套话是正常的）

use serde::{Deserialize, Serialize};

use super::dictionary::{ClicheCategory, ClicheDictionary, ClicheHit};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterAiScore {
    /// 0-100；越高越像人写的（越不像 AI）。
    pub humanity: u8,
    /// 命中数量（按 phrase 去重后的条目数）。
    pub distinct_hits: usize,
    /// 命中总次数（每行多次出现累加）。
    pub total_occurrences: u32,
    /// 按分类聚合。
    pub by_category: Vec<(ClicheCategory, u32)>,
    /// 详细命中清单。
    pub hits: Vec<ClicheHit>,
}

pub fn score_chapter(text: &str, dict: &ClicheDictionary) -> ChapterAiScore {
    let hits = dict.scan(text);
    let total_occurrences: u32 = hits.iter().map(|h| h.count).sum();
    let mut raw_penalty: i32 = 0;
    let mut by_category: std::collections::BTreeMap<ClicheCategory, u32> =
        std::collections::BTreeMap::new();
    for h in &hits {
        let weight: i32 = match h.entry.severity {
            1 => 1,
            2 => 2,
            3 => 4,
            _ => 1,
        };
        raw_penalty += weight as i32 * h.count as i32;
        *by_category.entry(h.entry.category).or_insert(0) += h.count;
    }
    // 长度因子：3000 字内不稀释；超过 3000 字，按 sqrt 放宽容忍度。
    let len = text.chars().count() as f32;
    let length_factor = if len <= 3000.0 {
        1.0
    } else {
        (3000.0 / len).sqrt()
    };
    let penalty = (raw_penalty as f32 * length_factor) as i32;
    let humanity = (100 - penalty).clamp(0, 100) as u8;

    ChapterAiScore {
        humanity,
        distinct_hits: hits.len(),
        total_occurrences,
        by_category: by_category.into_iter().collect(),
        hits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_scores_100() {
        let dict = ClicheDictionary::builtin();
        let text = "林羽拔出长刀，一刀劈向黑影。刀锋破空，那黑影应声而退。";
        let score = score_chapter(text, &dict);
        assert_eq!(score.humanity, 100);
    }

    #[test]
    fn heavy_cliche_drops_humanity() {
        let dict = ClicheDictionary::builtin();
        let text = "他的眼神变得深邃，心中涌起一股说不清的感觉，似乎又仿佛命中注定。";
        let score = score_chapter(text, &dict);
        assert!(score.humanity < 100);
        assert!(score.distinct_hits > 0);
    }
}

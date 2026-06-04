//! 题材专有的「禁用套话」黑名单。
//!
//! 例如：玄幻里「仿佛」「心中涌起」已经烂大街；言情里对男主外貌的
//! 「刀削般的脸」描述是 AI 标志；悬疑里滥用「种种迹象表明」会让
//! 读者出戏。

use serde::{Deserialize, Serialize};

use crate::deai::ClicheEntry;
use crate::model::project::Genre;

/// 一条黑名单命中。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClichéMatch {
    pub phrase: String,
    pub count: u32,
    pub first_line: Option<u32>,
}

/// 题材专有的「禁用套话」黑名单。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClichéBlacklist {
    entries: Vec<ClicheEntry>,
}

impl ClichéBlacklist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_genre(genre: Genre) -> Self {
        let mut b = Self::default();
        for e in builtin_for_genre(genre) {
            b.entries.push(e);
        }
        b
    }

    pub fn entries(&self) -> &[ClicheEntry] {
        &self.entries
    }

    pub fn scan(&self, text: &str) -> Vec<ClichéMatch> {
        use std::collections::BTreeMap;
        let mut by_phrase: BTreeMap<String, ClichéMatch> = BTreeMap::new();
        for (idx, line) in text.lines().enumerate() {
            for entry in &self.entries {
                if line.contains(&entry.phrase) {
                    let m = by_phrase
                        .entry(entry.phrase.clone())
                        .or_insert_with(|| ClichéMatch {
                            phrase: entry.phrase.clone(),
                            count: 0,
                            first_line: None,
                        });
                    m.count += line.match_indices(&entry.phrase).count() as u32;
                    if m.first_line.is_none() {
                        m.first_line = Some((idx + 1) as u32);
                    }
                }
            }
        }
        by_phrase.into_values().collect()
    }
}

fn entry(phrase: &str, reason: &str, suggestion: &str) -> ClicheEntry {
    ClicheEntry {
        phrase: phrase.into(),
        category: crate::deai::ClicheCategory::AbstractDescription,
        severity: 2,
        reason: reason.into(),
        suggestion: suggestion.into(),
    }
}

fn builtin_for_genre(genre: Genre) -> Vec<ClicheEntry> {
    use Genre::*;
    match genre {
        Xianxia => vec![
            entry("逆天而行", "烂大街的玄幻哲学", "换成具体的代价或规则"),
            entry("天赋异禀", "空洞的夸赞", "改成具体能力 + 一次展示"),
            entry("一代天骄", "老派玄幻形容", "删除"),
            entry("丹田", "玄学名词本身没问题，但经常被 LLM 当万能设定", "明确境界 → 丹田 → 经脉的对应关系"),
        ],
        Urban => vec![
            entry("高富帅", "都市文里过度套用", "换成具体的职级 / 资产 / 兴趣"),
            entry("白富美", "同上", "同上"),
            entry("成功人士", "空泛标签", "给具体身份与具体行为"),
        ],
        SciFi => vec![
            entry("高维", "科幻黑话", "用具体的维度 / 拓扑概念替代"),
            entry("量子", "量子就是用来滥用的", "如果不是真的涉及量子效应，就别用"),
        ],
        Fantasy => vec![
            entry("勇者", "西幻陈词", "换成具体的称号 / 头衔 / 事迹"),
        ],
        Historical => vec![
            entry("山呼万岁", "古装剧套词", "改成符合朝代的礼仪"),
            entry("圣上", "过于程式化", "结合身份使用更具体的称谓"),
        ],
        Romance => vec![
            entry("刀削般的脸", "AI 男主外貌标志", "换成具体的五官 / 动作 / 细节"),
            entry("心动的瞬间", "空洞", "落到具体事件"),
            entry("少女心", "AI 心理描写", "换成具体的身体反应"),
        ],
        Suspense => vec![
            entry("种种迹象表明", "AI 旁白", "改成具体的证据列举"),
            entry("冥冥之中", "悬疑玄学化", "删除"),
        ],
        Other => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blacklist_finds_genre_specific_phrase() {
        let bl = ClichéBlacklist::for_genre(Genre::Romance);
        let text = "他有一张刀削般的脸，让女主心动的瞬间发生了。";
        let hits = bl.scan(text);
        assert!(hits.iter().any(|m| m.phrase == "刀削般的脸"));
    }

    #[test]
    fn empty_for_other() {
        let bl = ClichéBlacklist::for_genre(Genre::Other);
        assert!(bl.entries().is_empty());
    }
}

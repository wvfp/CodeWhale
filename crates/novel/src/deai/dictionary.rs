//! 套话词典。
//!
//! 每条 [`ClicheEntry`] 都打上分类 + 严重度。严重度越高，扣分越多。
//! 「建议替换」是给 LLM 看的提示，不强制替换，只是告诉它「这里有 AI 味」。
//!
//! 数据按主题分组排列，方便后续增删。

use serde::{Deserialize, Serialize};

/// 套话分类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ClicheCategory {
    /// 抽象描写（眼神深邃、心中涌起）。
    AbstractDescription,
    /// 转折 / 句式套话（然而、却、可是）。
    Transition,
    /// 公式化对话（我不知道 / 我该怎么办）。
    FormulaicDialogue,
    /// 排比 / 对仗过度。
    ExcessiveParallelism,
    /// 抽签式哲理 / 鸡汤。
    PhilosophicCliché,
    /// 解释 / 旁白堆砌。
    OverExplanation,
    /// 视角穿越（突然跳出角色视角总结）。
    POVLeak,
    /// 副词滥用（很 / 非常 / 极其）。
    AdverbOveruse,
}

impl ClicheCategory {
    pub fn label(self) -> &'static str {
        match self {
            ClicheCategory::AbstractDescription => "抽象描写",
            ClicheCategory::Transition => "转折套话",
            ClicheCategory::FormulaicDialogue => "公式化对话",
            ClicheCategory::ExcessiveParallelism => "排比过度",
            ClicheCategory::PhilosophicCliché => "鸡汤哲理",
            ClicheCategory::OverExplanation => "过度旁白",
            ClicheCategory::POVLeak => "视角穿越",
            ClicheCategory::AdverbOveruse => "副词滥用",
        }
    }
}

/// 一条套话。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClicheEntry {
    /// 触发短语。
    pub phrase: String,
    /// 分类。
    pub category: ClicheCategory,
    /// 严重度（1-3）。1 = 轻，2 = 中，3 = 重。
    pub severity: u8,
    /// 简短中文解释，告诉 LLM 为什么这是套话。
    pub reason: String,
    /// 修改建议（给 LLM 看）。
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClicheHit {
    pub entry: ClicheEntry,
    pub count: u32,
    /// 第一次出现的行号（1-based），方便定位。
    pub first_line: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct ClicheDictionary {
    entries: Vec<ClicheEntry>,
}

impl ClicheDictionary {
    /// 内置一份常用词典（覆盖面足够 MVP 演示）。
    pub fn builtin() -> Self {
        let mut d = Self::default();
        d.push_many(builtin_entries());
        d
    }

    pub fn push(&mut self, entry: ClicheEntry) {
        self.entries.push(entry);
    }

    pub fn push_many(&mut self, entries: impl IntoIterator<Item = ClicheEntry>) {
        self.entries.extend(entries);
    }

    pub fn entries(&self) -> &[ClicheEntry] {
        &self.entries
    }

    /// 在文本中查找套话命中。返回的每个 [`ClicheHit`] 包含 phrase、出现次数、
    /// 第一次出现的行号。
    pub fn scan(&self, text: &str) -> Vec<ClicheHit> {
        use std::collections::BTreeMap;
        let mut by_phrase: BTreeMap<String, ClicheHit> = BTreeMap::new();
        for (idx, line) in text.lines().enumerate() {
            for entry in &self.entries {
                if line.contains(&entry.phrase) {
                    let hit = by_phrase
                        .entry(entry.phrase.clone())
                        .or_insert_with(|| ClicheHit {
                            entry: entry.clone(),
                            count: 0,
                            first_line: None,
                        });
                    hit.count += count_occurrences(line, &entry.phrase);
                    if hit.first_line.is_none() {
                        hit.first_line = Some((idx + 1) as u32);
                    }
                }
            }
        }
        by_phrase.into_values().collect()
    }
}

fn count_occurrences(haystack: &str, needle: &str) -> u32 {
    if needle.is_empty() {
        return 0;
    }
    haystack.match_indices(needle).count() as u32
}

fn builtin_entries() -> Vec<ClicheEntry> {
    use ClicheCategory::*;
    vec![
        // === 抽象描写 ===
        entry("眼神变得深邃", AbstractDescription, 3, "AI 味最重的描写之一",
              "换成具体动作或物件，例如「他盯着烛火，把手按在剑柄上」。"),
        entry("心中涌起一股", AbstractDescription, 3, "典型 AI 抒情模板",
              "用身体反应替代：「胸口发紧 / 手心出汗 / 喉咙干涩」。"),
        entry("心中升起一阵", AbstractDescription, 3, "同上的 AI 抒情模板",
              "改成具体的身体感受或环境反应。"),
        entry("仿佛", AbstractDescription, 2, "网络小说里常常被过度使用",
              "如果后接的是直观类比可保留，否则换成「像 / 就好像」。"),
        entry("宛如", AbstractDescription, 2, "老派抒情词，网文里很 AI",
              "换成更具体的比喻或直接写感受。"),
        entry("似乎", AbstractDescription, 1, "软化判断",
              "如果不是有意营造模糊感，可以直接断言。"),
        // === 转折套话 ===
        entry("然而", Transition, 1, "非常常见的转折词，过度使用显得 AI",
              "考虑删除、换成「可是 / 不过 / 而 / 但」，或拆成两句。"),
        entry("却", Transition, 1, "用得太多会显得 LLM",
              "能删则删；不能删的可以改用「偏偏 / 偏 / 谁知」。"),
        // === 公式化对话 ===
        entry("我不知道", FormulaicDialogue, 2, "角色最常被 LLM 给的台词之一",
              "用具体疑问替代：「他真的会来吗 / 我们还能回得去吗」。"),
        entry("我该怎么办", FormulaicDialogue, 3, "经典 AI 角色台词",
              "用具体的行动意向替代：「先去禀报师父 / 把这件事压下去」。"),
        entry("这一切都是命中注定", PhilosophicCliché, 3, "AI 鸡汤",
              "删除，或者换成角色个人化的感慨。"),
        entry("命运的车轮", PhilosophicCliché, 3, "陈词滥调",
              "删除整段。"),
        // === 排比 / 哲理 ===
        entry("种种迹象表明", OverExplanation, 2, "AI 旁白惯用",
              "直接写「她注意到了 X / Y / Z 三件事」。"),
        entry("不难看出", OverExplanation, 2, "AI 旁白惯用",
              "删除，保留事实即可。"),
        entry("这让他明白了一个道理", PhilosophicCliché, 3, "AI 总结句",
              "删除；让行动 / 结果说话。"),
        // === 副词滥用 ===
        entry("非常", AdverbOveruse, 1, "副词替代具体描写",
              "换成具体的动作或程度描写。"),
        entry("极其", AdverbOveruse, 1, "副词替代具体描写",
              "换成具体的动作或程度描写。"),
        entry("无比", AdverbOveruse, 1, "副词替代具体描写",
              "换成具体的动作或程度描写。"),
    ]
}

fn entry(
    phrase: &str,
    category: ClicheCategory,
    severity: u8,
    reason: &str,
    suggestion: &str,
) -> ClicheEntry {
    ClicheEntry {
        phrase: phrase.to_string(),
        category,
        severity,
        reason: reason.to_string(),
        suggestion: suggestion.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_finds_known_cliche() {
        let d = ClicheDictionary::builtin();
        let text = "他的眼神变得深邃，心中涌起一股说不清的感觉。";
        let hits = d.scan(text);
        assert!(!hits.is_empty());
        let phrases: Vec<&str> = hits.iter().map(|h| h.entry.phrase.as_str()).collect();
        assert!(phrases.contains(&"眼神变得深邃"));
        assert!(phrases.contains(&"心中涌起一股"));
    }

    #[test]
    fn scan_aggregates_multiple_occurrences() {
        let d = ClicheDictionary::builtin();
        let text = "他非常惊讶。\n她非常生气。\n它非常危险。\n";
        let hits = d.scan(text);
        let hit = hits.iter().find(|h| h.entry.phrase == "非常").unwrap();
        assert_eq!(hit.count, 3);
        assert_eq!(hit.first_line, Some(1));
    }
}

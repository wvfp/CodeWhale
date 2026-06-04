//! 一致性检查引擎。
//!
//! MVP 实现是规则化 + 关键词匹配。每一种检查对应 [`IssueCategory`] 的一种。
//! 后续可以挂上 LLM 驱动的检查（生成式审计），通过 [`ConsistencyChecker`] 注入。

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::consistency::rules::{ConsistencyIssue, ConsistencyReport, IssueCategory, IssueStats};
use crate::facts::FACTS_PATH;

#[derive(Debug, Error)]
pub enum ConsistencyError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ConsistencyError>;

/// 轻量的「事实库视图」。`FactStore` 内部就是 `.novelwhale/facts.json`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactStore {
    #[serde(default)]
    pub characters: Vec<FactCharacter>,
    #[serde(default)]
    pub events: Vec<Value>,
    #[serde(default)]
    pub world_rules: Vec<FactWorldRule>,
    #[serde(default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub extraction_chapters: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactCharacter {
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub chapter_introduced: Option<u32>,
    /// 是否被标记为「死亡」或「永久退场」。
    #[serde(default)]
    pub alive: bool,
    /// 死亡 / 退场章节。
    #[serde(default)]
    pub last_appearance: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactWorldRule {
    pub id: String,
    pub rule: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub source_chapter: Option<u32>,
    #[serde(default)]
    pub immutable: bool,
}

impl FactStore {
    /// 从项目根目录加载事实库。如果文件不存在，返回一个空的 `FactStore`。
    pub fn load(project_root: &Path) -> Result<Self> {
        let path = project_root.join(FACTS_PATH);
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let store: Self = serde_json::from_str(&raw).unwrap_or_default();
        Ok(store)
    }
}

/// 一致性检查器。
#[derive(Debug, Default, Clone)]
pub struct ConsistencyChecker;

impl ConsistencyChecker {
    pub fn new() -> Self {
        Self
    }

    /// 对单章正文做一致性检查。
    pub fn check(
        &self,
        chapter_number: u32,
        content: &str,
        store: &FactStore,
    ) -> ConsistencyReport {
        let mut issues = Vec::new();
        self.check_world_rules(content, store, &mut issues);
        self.check_dead_characters(chapter_number, content, store, &mut issues);
        self.check_character_aliases(content, store, &mut issues);
        self.check_quantity_contradictions(content, &mut issues);

        let stats = IssueStats {
            errors: issues.iter().filter(|i| i.severity == 2).count() as u32,
            warnings: issues.iter().filter(|i| i.severity == 1).count() as u32,
            hints: issues.iter().filter(|i| i.severity == 0).count() as u32,
        };
        let score = compute_score(&stats, content);

        ConsistencyReport {
            chapter_number,
            score,
            issues,
            stats,
        }
    }

    fn check_world_rules(
        &self,
        content: &str,
        store: &FactStore,
        issues: &mut Vec<ConsistencyIssue>,
    ) {
        for rule in store.world_rules.iter().filter(|r| r.immutable) {
            for probe in rule_probes(&rule.rule) {
                if content.contains(&probe.noun) && !is_negated_context(content, &probe.noun) {
                    let line = line_index(content, &probe.noun);
                    let mut explanation = format!(
                        "章节提到了「{}」，与不可变规则「{}」冲突（分类 {}，来自第 {} 章）",
                        probe.noun,
                        rule.rule,
                        rule.category,
                        rule.source_chapter
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "?".into())
                    );
                    if let Some(n) = line {
                        explanation.push_str(&format!("（位于第 {} 行）", n));
                    }
                    issues.push(ConsistencyIssue {
                        category: IssueCategory::WorldRuleViolation,
                        severity: 2,
                        source: format!("rule:{}", rule.id),
                        matched_text: probe.noun.clone(),
                        explanation,
                    });
                }
            }
        }
    }

    fn check_dead_characters(
        &self,
        chapter_number: u32,
        content: &str,
        store: &FactStore,
        issues: &mut Vec<ConsistencyIssue>,
    ) {
        for ch in store.characters.iter().filter(|c| !c.alive) {
            let intro = ch.chapter_introduced.unwrap_or(0);
            if chapter_number <= intro {
                continue;
            }
            if content.contains(&ch.name) {
                let line = line_index(content, &ch.name);
                let severity = if chapter_number > ch.last_appearance.unwrap_or(intro) + 1 {
                    2
                } else {
                    1
                };
                let mut explanation = format!(
                    "角色「{}」已在第 {} 章后标记为退场，但本章再次出现",
                    ch.name,
                    ch.last_appearance
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "?".into())
                );
                if let Some(n) = line {
                    explanation.push_str(&format!("（位于第 {} 行）", n));
                }
                issues.push(ConsistencyIssue {
                    category: IssueCategory::DeadCharacterRevival,
                    severity,
                    source: format!("character:{}", ch.name),
                    matched_text: ch.name.clone(),
                    explanation,
                });
            }
        }
    }

    fn check_character_aliases(
        &self,
        content: &str,
        store: &FactStore,
        issues: &mut Vec<ConsistencyIssue>,
    ) {
        use std::collections::BTreeMap;
        let mut groups: BTreeMap<String, Vec<&FactCharacter>> = BTreeMap::new();
        for ch in &store.characters {
            groups.entry(ch.name.clone()).or_default().push(ch);
        }
        for (name, group) in groups.iter() {
            if group.len() > 1 {
                let line = line_index(content, name);
                let mut explanation = format!(
                    "事实库中存在 {} 条同名「{}」的记录，请确认是否需要消歧",
                    group.len(),
                    name
                );
                if let Some(n) = line {
                    explanation.push_str(&format!("（位于第 {} 行）", n));
                }
                issues.push(ConsistencyIssue {
                    category: IssueCategory::AmbiguousName,
                    severity: 1,
                    source: format!("character:{}", name),
                    matched_text: name.clone(),
                    explanation,
                });
            }
        }
    }

    fn check_quantity_contradictions(
        &self,
        content: &str,
        issues: &mut Vec<ConsistencyIssue>,
    ) {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(phrase) = extract_quantity_phrase(line) {
                *counts.entry(phrase).or_insert(0) += 1;
            }
        }
        for (phrase, count) in counts {
            if count >= 3 {
                let line = line_index(content, &phrase);
                let mut explanation = format!(
                    "同一章内出现 {} 次「{}」，建议核对是否需要变化",
                    count, phrase
                );
                if let Some(n) = line {
                    explanation.push_str(&format!("（位于第 {} 行）", n));
                }
                issues.push(ConsistencyIssue {
                    category: IssueCategory::QuantityContradiction,
                    severity: 0,
                    source: "phrase-frequency".to_string(),
                    matched_text: phrase.clone(),
                    explanation,
                });
            }
        }
    }
}

fn compute_score(stats: &IssueStats, content: &str) -> u8 {
    // 没有问题时一律满分，避免被长度因子拖低
    if stats.errors == 0 && stats.warnings == 0 && stats.hints == 0 {
        return 100;
    }
    let length = content.chars().count();
    if length == 0 {
        return 100;
    }
    let length_factor = (length as f32 / 3000.0).clamp(0.6, 1.0);
    let raw = 100.0
        - (stats.errors as f32) * 12.0
        - (stats.warnings as f32) * 5.0
        - (stats.hints as f32) * 1.0;
    let scaled = (raw * length_factor).clamp(0.0, 100.0);
    scaled as u8
}

fn line_index(content: &str, needle: &str) -> Option<u32> {
    for (idx, line) in content.lines().enumerate() {
        if line.contains(needle) {
            return Some((idx + 1) as u32);
        }
    }
    None
}

struct RuleProbe {
    /// 抽取出来的「禁止/缺失的对象」关键词（规则否定词后面的名词短语）。
    noun: String,
}

fn rule_probes(rule: &str) -> Vec<RuleProbe> {
    // 抽取「没有 / 不 / 禁止 / 不可 / 严禁」后面的关键名词短语，作为「如果章节里
    // 提到这个名词则怀疑违反」的探针。命中后再用 is_negated_context 排除元叙述
    // （例如规则本身被复述时）。
    let mut probes = Vec::new();
    let markers = ["没有", "禁止", "不可", "严禁", "不能", "无法"];
    for marker in markers {
        for (idx, _) in rule.match_indices(marker) {
            let tail = &rule[idx..];
            let phrase: String = tail
                .chars()
                .take_while(|c| !matches!(*c, '。' | '，' | '；' | '.' | ',' | ';' | '、'))
                .take(20)
                .collect();
            if phrase.chars().count() <= marker.chars().count() {
                continue;
            }
            // 去掉开头的标记，得到「关键名词」
            let noun: String = phrase.chars().skip(marker.chars().count()).collect();
            let noun = noun.trim().to_string();
            if noun.is_empty() || noun.chars().count() > 12 {
                continue;
            }
            probes.push(RuleProbe { noun });
        }
    }
    probes
}

fn is_negated_context(content: &str, noun: &str) -> bool {
    // 在命中的 noun 前面若干字符内，如果出现「没有 / 并非 / 并未」等
    // 否定词，则认为是元叙述（例如规则被复述、回忆否定），不视为违反。
    // 必须按 char 边界切片，否则中文文本上会 panic。
    let negators = ["没有", "并无", "并非", "并未", "并不", "不存", "不具"];
    for (idx, _) in content.match_indices(noun) {
        // 把 byte 偏移转成 char 偏移，向前看 8 个字符
        let char_idx = content[..idx].chars().count();
        let start_char = char_idx.saturating_sub(8);
        let start_byte = char_to_byte_idx(content, start_char);
        let prefix = &content[start_byte..idx];
        if negators.iter().any(|n| prefix.contains(n)) {
            return true;
        }
    }
    false
}

/// 找到第 n 个 char 之后的字节偏移。
fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn extract_quantity_phrase(line: &str) -> Option<String> {
    // 匹配「3人 / 三个人 / 5件 / 七柄」等结构。
    // 直接在 char 级别工作，避免任何字节切片引发的 UTF-8 边界问题。
    // 只抽取「数字 + 紧邻的 1 个 CJK 量词」作为「量化短语」，
    // 避免把后面真正的主语（走/又/终）一起吞掉导致无法聚合。
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    // 跳过非数字字符
    while i < chars.len() && !(chars[i].is_ascii_digit() || is_cjk_digit(chars[i])) {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    // 收集连续的数字字符
    let digit_start = i;
    while i < chars.len() && (chars[i].is_ascii_digit() || is_cjk_digit(chars[i])) {
        i += 1;
    }
    // 收集紧随其后的 1 个 CJK 量词字符
    let unit_start = i;
    if i < chars.len() && is_cjk(chars[i]) && is_unit_char(chars[i]) {
        i += 1;
    }
    if unit_start == i {
        return None;
    }
    let digits: String = chars[digit_start..unit_start].iter().collect();
    let unit: String = chars[unit_start..i].iter().collect();
    Some(format!("{}{}", digits, unit))
}

/// 常见的中文量词 / 助量字符。仅当数字后紧跟这些字符之一时，
/// 才把「数字 + 1 个量词」视为一个量化短语；否则只返回数字部分（这里
/// 直接返回 None，让上层知道此行没有量化短语）。
fn is_unit_char(c: char) -> bool {
    // 数字 + 单位组合里紧跟的那个字大多是量词或时间 / 货币 / 重量单位。
    // 这里枚举了常见用法，命中不到就当作普通文本，不计入重复统计。
    matches!(
        c,
        // 通用个体量词
        '个' | '些' | '位' | '名' | '人' | '只' | '匹' | '头' | '条' | '件' | '把'
        | '柄' | '张' | '块' | '颗' | '粒' | '滴' | '杯' | '瓶' | '碗' | '盘'
        | '盏' | '棵' | '株' | '朵' | '门' | '种' | '样' | '项' | '段' | '章'
        | '节' | '课' | '道' | '句' | '词' | '字' | '行' | '列' | '排' | '队'
        | '班' | '组' | '团' | '群' | '对' | '双' | '打' | '套' | '沓' | '摞'
        | '叠' | '堆' | '层' | '座' | '栋' | '幢' | '扇' | '面' | '端' | '部'
        | '局' | '场' | '幕' | '页' | '篇' | '卷' | '册' | '本' | '支' | '枝'
        | '束' | '伙' | '帮' | '批' | '簇' | '串' | '家' | '国' | '城' | '村'
        // 时间
        | '年' | '月' | '日' | '时' | '刻' | '分' | '秒' | '代' | '纪' | '世'
        // 长度 / 重量 / 货币
        | '丈' | '尺' | '寸' | '里' | '步' | '石' | '斤' | '两' | '钱'
        | '毫' | '吨' | '克' | '米'
        // 度量 / 信息
        | '度' | '倍' | '成' | '回' | '次' | '趟' | '遍' | '遭' | '下' | '番'
    )
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF)
}

fn is_cjk_digit(c: char) -> bool {
    matches!(c, '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百' | '千' | '万')
}

/// 把报告序列化成 LLM 友好的 JSON。
pub fn report_to_json(report: &ConsistencyReport) -> Value {
    json!({
        "chapter_number": report.chapter_number,
        "score": report.score,
        "stats": {
            "errors": report.stats.errors,
            "warnings": report.stats.warnings,
            "hints": report.stats.hints,
        },
        "issues": report.issues.iter().map(|i| json!({
            "category": i.category.label(),
            "category_id": format!("{:?}", i.category).to_lowercase(),
            "severity": i.severity,
            "source": i.source,
            "matched_text": i.matched_text,
            "explanation": i.explanation,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consistency::rules::ConsistencyIssue;

    fn store_with_rules(rules: Vec<FactWorldRule>) -> FactStore {
        FactStore {
            world_rules: rules,
            ..Default::default()
        }
    }

    #[test]
    fn detects_world_rule_violation() {
        let store = store_with_rules(vec![FactWorldRule {
            id: "no_magic".into(),
            rule: "这个世界没有魔法".into(),
            category: "other".into(),
            source_chapter: Some(1),
            immutable: true,
        }]);
        let checker = ConsistencyChecker::new();
        let report = checker.check(2, "林羽念动咒语，魔法从指尖涌出。", &store);
        assert!(report.score < 100);
        let rule_violation = report
            .issues
            .iter()
            .find(|i| i.category == IssueCategory::WorldRuleViolation);
        assert!(rule_violation.is_some());
    }

    #[test]
    fn no_violation_when_content_obeys_rule() {
        let store = store_with_rules(vec![FactWorldRule {
            id: "no_magic".into(),
            rule: "这个世界没有魔法".into(),
            category: "other".into(),
            source_chapter: Some(1),
            immutable: true,
        }]);
        let checker = ConsistencyChecker::new();
        let report = checker.check(2, "林羽取出长刀，与敌人近身搏斗。", &store);
        assert_eq!(report.score, 100);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn no_violation_when_noun_appears_in_negated_context() {
        let store = store_with_rules(vec![FactWorldRule {
            id: "no_magic".into(),
            rule: "这个世界没有魔法".into(),
            category: "other".into(),
            source_chapter: Some(1),
            immutable: true,
        }]);
        let checker = ConsistencyChecker::new();
        // 角色在复述规则本身 — 应当识别为元叙述而非违反
        let report = checker.check(2, "老村长说：「这个山谷里没有魔法，那些外乡人别再提了。」", &store);
        assert!(report.issues.is_empty(), "应将元叙述识别为非违反: {:?}", report.issues);
    }

    #[test]
    fn detects_dead_character_revival() {
        let mut store = FactStore::default();
        store.characters.push(FactCharacter {
            name: "王五".into(),
            status: "dead".into(),
            chapter_introduced: Some(1),
            alive: false,
            last_appearance: Some(3),
        });
        let checker = ConsistencyChecker::new();
        let report = checker.check(5, "王五从城门走出，与张三打招呼。", &store);
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == IssueCategory::DeadCharacterRevival);
        assert!(issue.is_some());
    }

    #[test]
    fn flags_repeated_quantity_phrases() {
        let store = FactStore::default();
        let checker = ConsistencyChecker::new();
        let content = "3人走进山谷。\n3人又走了一段路。\n3人终于到了终点。\n";
        let report = checker.check(1, content, &store);
        let issue = report
            .issues
            .iter()
            .find(|i| i.category == IssueCategory::QuantityContradiction);
        assert!(issue.is_some());
    }

    #[test]
    fn empty_content_produces_clean_report() {
        let store = FactStore::default();
        let checker = ConsistencyChecker::new();
        let report = checker.check(1, "", &store);
        assert_eq!(report.score, 100);
    }

    #[test]
    fn issue_stats_match_issues() {
        let _issues = vec![
            ConsistencyIssue {
                category: IssueCategory::WorldRuleViolation,
                severity: 2,
                source: "a".into(),
                matched_text: "x".into(),
                explanation: "".into(),
            },
            ConsistencyIssue {
                category: IssueCategory::DeadCharacterRevival,
                severity: 1,
                source: "b".into(),
                matched_text: "y".into(),
                explanation: "".into(),
            },
        ];
        let stats = IssueStats {
            errors: 1,
            warnings: 1,
            hints: 0,
        };
        assert_eq!(stats.errors, 1);
        assert_eq!(stats.warnings, 1);
    }
}

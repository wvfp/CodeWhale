//! 类型化的一致性规则。
//!
//! 每种规则在 [`ConsistencyChecker`] 里有一种对应的检查实现。
//! 新增规则时，请同时在 `tools/consistency_check.rs` 的工具描述里登记。

use serde::{Deserialize, Serialize};

/// 一条具体的一致性问题。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsistencyIssue {
    /// 问题分类（用于聚合 + UI 展示）。
    pub category: IssueCategory,
    /// 严重程度：0 = 提示，1 = 警告，2 = 错误。
    pub severity: u8,
    /// 触发该问题的规则来源（事实 key、角色名、事件 id 等）。
    pub source: String,
    /// 在章节正文中命中的短语（用于定位）。
    pub matched_text: String,
    /// 解释问题的人话（中文）。
    pub explanation: String,
}

/// 问题分类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IssueCategory {
    /// 与已锁定的世界规则相矛盾。
    WorldRuleViolation,
    /// 已死亡 / 已消失的角色再次出现。
    DeadCharacterRevival,
    /// 时间线错乱（先发生的事被当作成已经发生）。
    TimelineContradiction,
    /// 角色外形 / 性格描述与已建立的事实不符。
    CharacterDescriptionDrift,
    /// 数字 / 量词前后不一致。
    QuantityContradiction,
    /// 同名角色被误用。
    AmbiguousName,
}

impl IssueCategory {
    pub fn label(self) -> &'static str {
        match self {
            IssueCategory::WorldRuleViolation => "世界规则违反",
            IssueCategory::DeadCharacterRevival => "已死亡角色复活",
            IssueCategory::TimelineContradiction => "时间线矛盾",
            IssueCategory::CharacterDescriptionDrift => "角色设定漂移",
            IssueCategory::QuantityContradiction => "数字/量词矛盾",
            IssueCategory::AmbiguousName => "角色名歧义",
        }
    }
}

/// 一致性检查报告。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsistencyReport {
    pub chapter_number: u32,
    /// 0-100；100 表示没有发现问题。
    pub score: u8,
    /// 全部问题（按严重程度降序）。
    pub issues: Vec<ConsistencyIssue>,
    /// 按分类聚合的统计。
    pub stats: IssueStats,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IssueStats {
    pub errors: u32,
    pub warnings: u32,
    pub hints: u32,
}

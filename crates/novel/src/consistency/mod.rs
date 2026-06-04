//! 一致性检查模块。
//!
//! 由 [`ConsistencyChecker`]（[`checker`]）对单章正文跑一组规则（[`rules`]），
//! 输出 [`ConsistencyReport`]。当前实现是规则化 + 关键词匹配；可由调用方注入
//! 不同的 checker 来启用更复杂的 LLM 驱动检查。

pub mod checker;
pub mod rules;

pub use checker::{
    ConsistencyChecker, ConsistencyError, FactCharacter, FactStore, FactWorldRule, Result,
    report_to_json,
};
pub use rules::{ConsistencyIssue, ConsistencyReport, IssueCategory, IssueStats};

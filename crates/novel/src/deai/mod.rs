//! 去 AI 化引擎。
//!
//! 网文读者对「AI 味」非常敏感：过度套用「仿佛 / 似乎 / 心中涌起 / 眼神
//! 变得深邃」等模板短语、滥用「然而 / 却」转折、公式化「我不知道 / 我
//! 应该怎么办」、大量对仗工整但无信息量的排比 — 都是 LLM 痕迹。
//!
//! 本模块提供：
//!
//! 1. [`dictionary::ClicheDictionary`] — 套话词典（含分类、严重度）。
//! 2. [`scoring::score_chapter`] — 对单章跑一次「AI 味」打分。
//! 3. [`suggestions::build_suggestions`] — 把命中改写成「给模型的修改指令」，
//!    让模型自己重写而不是机械替换。
//!
//! 所有实现都是规则化的、不依赖外部服务，方便集成。

pub mod dictionary;
pub mod scoring;
pub mod suggestions;
pub mod types;

pub use dictionary::{ClicheCategory, ClicheEntry, ClicheHit, ClicheDictionary};
pub use scoring::{score_chapter, ChapterAiScore};
pub use suggestions::{build_suggestions, Suggestion};
pub use types::{DeAiError, DeAiResult};

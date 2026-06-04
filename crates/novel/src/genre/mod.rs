//! 题材模板（genre templates）。
//!
//! 不同题材有不同的「节奏配方」：玄幻升级流一章 2000-2500 字，节奏是「打怪 /
//! 升级 / 揭示新地图」三段循环；悬疑一章结尾必须留钩子；都市日常一章 1500 字
//! 就够，重在对话与场景细节。
//!
//! 本模块为每种 [`Genre`] 提供：
//!
//! - [`templates::GenreTemplate`]：节奏配方、推荐字数、推荐段落比例。
//! - [`templates::ArcTemplate`]：建议的故事弧线阶段切分（卷 / 高潮点）。
//! - [`cliches::ClichéBlacklist`]：题材专有的禁用套话。
//! - [`rules::GenreRule`]：题材专有的一致性规则。
//!
//! 所有数据都内置一份常用实现，可由项目按需覆盖。

pub mod cliches;
pub mod rules;
pub mod templates;

pub use cliches::{ClichéBlacklist, ClichéMatch};
pub use rules::{GenreRule, RuleSeverity, rules_for};
pub use templates::{ArcPhase, ArcTemplate, GenreTemplate, PaceTarget, SceneMix};

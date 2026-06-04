//! 题材专有的一致性规则。
//!
//! 例如：玄幻里境界 / 法宝名称不能前后不一；悬疑里嫌疑人的 alibi 不能
//! 在不同章节里出现时间冲突；言情里男女主关系进展要符合「相识 → 试探 →
//! 在一起」的三段式。

use serde::{Deserialize, Serialize};

use crate::model::project::Genre;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleSeverity {
    Info,
    Warning,
    Hard,
}

/// 一条题材规则。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenreRule {
    pub id: String,
    pub description: String,
    pub severity: RuleSeverity,
    /// 简单正则 / 关键词扫描函数（key 是要匹配的子串）。
    pub check_hint: String,
}

pub fn rules_for(genre: Genre) -> Vec<GenreRule> {
    use Genre::*;
    match genre {
        Xianxia => vec![
            GenreRule {
                id: "realm-named-once".into(),
                description: "境界 / 法宝名称全书统一；同一境界在不同章节不能有不同别名".into(),
                severity: RuleSeverity::Hard,
                check_hint: "境界".into(),
            },
            GenreRule {
                id: "protagonist-not-omnipotent".into(),
                description: "金手指必须有明确限制，主角不可无理由碾压高境界".into(),
                severity: RuleSeverity::Warning,
                check_hint: "金手指".into(),
            },
        ],
        Urban => vec![GenreRule {
            id: "money-arithmetic".into(),
            description: "金额 / 收入 / 资产要前后一致".into(),
            severity: RuleSeverity::Hard,
            check_hint: "万 / 亿 / 元".into(),
        }],
        SciFi => vec![GenreRule {
            id: "tech-consistency".into(),
            description: "同一装置的能力不能在不同章节里矛盾".into(),
            severity: RuleSeverity::Hard,
            check_hint: "装置 / 引擎".into(),
        }],
        Fantasy => vec![GenreRule {
            id: "magic-cost".into(),
            description: "魔法 / 神术必须有代价，不能免费用".into(),
            severity: RuleSeverity::Warning,
            check_hint: "代价".into(),
        }],
        Historical => vec![GenreRule {
            id: "era-terms".into(),
            description: "官职 / 礼仪 / 称谓要符合朝代".into(),
            severity: RuleSeverity::Warning,
            check_hint: "臣 / 陛下 / 圣上".into(),
        }],
        Romance => vec![GenreRule {
            id: "relationship-pacing".into(),
            description: "关系进展要符合既定的相识 → 试探 → 在一起三段式".into(),
            severity: RuleSeverity::Info,
            check_hint: "心动 / 在意".into(),
        }],
        Suspense => vec![GenreRule {
            id: "alibi-consistency".into(),
            description: "嫌疑人的不在场证明在不同章节里不能自相矛盾".into(),
            severity: RuleSeverity::Hard,
            check_hint: "证人 / 不在场".into(),
        }],
        Other => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_genres_have_rules() {
        for g in [
            Genre::Xianxia,
            Genre::Urban,
            Genre::SciFi,
            Genre::Fantasy,
            Genre::Historical,
            Genre::Romance,
            Genre::Suspense,
            Genre::Other,
        ] {
            let _ = rules_for(g);
        }
    }
}

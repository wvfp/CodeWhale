//! 题材模板。
//!
//! 每种题材对应一份 [`GenreTemplate`]，里面写死：建议单章字数、场景比例、
//! 故事弧线阶段切分等。

use serde::{Deserialize, Serialize};

use crate::model::project::Genre;

/// 节奏目标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaceTarget {
    /// 建议单章字数（汉字计）。
    pub recommended_words: u32,
    /// 最低字数（低于此章会被节奏检查标红）。
    pub min_words: u32,
    /// 最高字数（超过此章可能需要拆章）。
    pub max_words: u32,
}

/// 场景比例（百分比，加起来应当 ≈100）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneMix {
    pub dialogue: u8,
    pub action: u8,
    pub description: u8,
    pub introspection: u8,
}

impl SceneMix {
    pub fn total(&self) -> u8 {
        self.dialogue + self.action + self.description + self.introspection
    }
}

/// 一段故事弧线（如「第 1 卷：开篇 + 试炼 + 小高潮」）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArcPhase {
    /// 阶段名。
    pub name: String,
    /// 阶段描述（给 LLM 看的）。
    pub goal: String,
    /// 在全卷中的章节占比（0-1）。
    pub weight: f32,
    /// 是否需要在本阶段结尾留「钩子」。
    pub requires_hook: bool,
}

/// 一个完整的故事弧线模板。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArcTemplate {
    /// 弧线总章节数（按此题材的常见卷长）。
    pub total_chapters: u32,
    /// 阶段列表。
    pub phases: Vec<ArcPhase>,
}

impl ArcTemplate {
    /// 给定一个章节号，返回它应该属于的阶段。
    pub fn phase_for_chapter(&self, chapter: u32) -> Option<&ArcPhase> {
        if self.phases.is_empty() {
            return None;
        }
        let ratio = (chapter as f32) / (self.total_chapters.max(1) as f32);
        let mut acc = 0.0_f32;
        for phase in &self.phases {
            acc += phase.weight;
            if ratio <= acc + 1e-6 {
                return Some(phase);
            }
        }
        self.phases.last()
    }
}

/// 题材模板 = 节奏 + 场景 + 弧线 + 关键提示。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenreTemplate {
    pub genre: Genre,
    /// 题材中文名。
    pub display_name: String,
    /// 节奏目标。
    pub pace: PaceTarget,
    /// 场景比例。
    pub scene_mix: SceneMix,
    /// 推荐弧线。
    pub arc: ArcTemplate,
    /// 题材要诀（给 LLM 看的几句话）。
    pub key_principles: Vec<String>,
    /// 常见爽点（每条作为对 LLM 的提示）。
    pub dopamine_moments: Vec<String>,
}

impl GenreTemplate {
    pub fn for_genre(genre: Genre) -> Self {
        match genre {
            Genre::Xianxia => xianxia_template(),
            Genre::Urban => urban_template(),
            Genre::SciFi => scifi_template(),
            Genre::Fantasy => fantasy_template(),
            Genre::Historical => historical_template(),
            Genre::Romance => romance_template(),
            Genre::Suspense => suspense_template(),
            Genre::Other => other_template(),
        }
    }
}

// === 各题材的实现 ===

fn xianxia_template() -> GenreTemplate {
    GenreTemplate {
        genre: Genre::Xianxia,
        display_name: "玄幻 / 仙侠".into(),
        pace: PaceTarget {
            recommended_words: 2200,
            min_words: 1800,
            max_words: 3000,
        },
        scene_mix: SceneMix {
            dialogue: 30,
            action: 35,
            description: 25,
            introspection: 10,
        },
        arc: ArcTemplate {
            total_chapters: 50,
            phases: vec![
                ArcPhase {
                    name: "开篇立人设".into(),
                    goal: "塑造主角，建立金手指/世界观，埋下第一个反派/机缘".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "小试牛刀".into(),
                    goal: "主角第一次实战胜利 / 第一次装逼".into(),
                    weight: 0.3,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "中段反转".into(),
                    goal: "出现比主角更强的敌人 / 失势".into(),
                    weight: 0.3,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "卷末高潮".into(),
                    goal: "突破境界 / 击败卷末Boss，开启新地图".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
            ],
        },
        key_principles: vec![
            "每章至少出现一次「爽点」：装逼打脸 / 反转 / 升级 / 奇遇".into(),
            "境界体系不可前后矛盾，跨级战斗必须给出明确理由".into(),
            "金手指能力要有限制，不能让主角无理由碾压".into(),
            "反派不能是纯粹的弱智；强敌须有强敌的样子".into(),
        ],
        dopamine_moments: vec![
            "路人甲讥讽 → 主角亮出真实身份".into(),
            "拍卖会 / 比武招亲上一鸣惊人".into(),
            "神秘老者 / 戒指老爷爷指点".into(),
            "看似柔弱的师妹其实是大佬".into(),
        ],
    }
}

fn urban_template() -> GenreTemplate {
    GenreTemplate {
        genre: Genre::Urban,
        display_name: "都市 / 日常".into(),
        pace: PaceTarget {
            recommended_words: 1800,
            min_words: 1500,
            max_words: 2500,
        },
        scene_mix: SceneMix {
            dialogue: 45,
            action: 15,
            description: 25,
            introspection: 15,
        },
        arc: ArcTemplate {
            total_chapters: 40,
            phases: vec![
                ArcPhase {
                    name: "立人设".into(),
                    goal: "介绍主角身份、处境、核心关系".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "日常推进".into(),
                    goal: "工作 / 感情 / 副业多线并行".into(),
                    weight: 0.5,
                    requires_hook: false,
                },
                ArcPhase {
                    name: "矛盾爆发".into(),
                    goal: "主角遭遇重大打击或反转".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "收尾 / 升级".into(),
                    goal: "主角重新站稳，开启新副本".into(),
                    weight: 0.1,
                    requires_hook: true,
                },
            ],
        },
        key_principles: vec![
            "人物对白占比高，对话要「能听出口音和身份」".into(),
            "金钱 / 职级 / 关系网等现实元素要前后一致".into(),
            "情感线要慢热，避免一见钟情式套话".into(),
        ],
        dopamine_moments: vec![
            "低调身份被当众揭穿".into(),
            "前任 / 旧识当场难堪".into(),
            "关键时刻谈下大单 / 救场".into(),
        ],
    }
}

fn scifi_template() -> GenreTemplate {
    GenreTemplate {
        genre: Genre::SciFi,
        display_name: "科幻".into(),
        pace: PaceTarget {
            recommended_words: 2200,
            min_words: 1800,
            max_words: 3000,
        },
        scene_mix: SceneMix {
            dialogue: 30,
            action: 25,
            description: 30,
            introspection: 15,
        },
        arc: ArcTemplate {
            total_chapters: 60,
            phases: vec![
                ArcPhase {
                    name: "设定建立".into(),
                    goal: "把世界规则 / 科技层级 / 主角处境交代清楚".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "事件升级".into(),
                    goal: "外部冲突逐步逼近，主角决策开始有代价".into(),
                    weight: 0.4,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "真相揭示".into(),
                    goal: "反派动机 / 阴谋全貌浮出水面".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "代价与希望".into(),
                    goal: "主角付出代价后达成新平衡".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
            ],
        },
        key_principles: vec![
            "科技名词要前后一致，不要混用时代名词".into(),
            "硬科幻尽量避免「毫无代价的奇迹」".into(),
            "宏大叙事要落到具体一个角色身上".into(),
        ],
        dopamine_moments: vec![
            "看似无关的小数据最终推翻了整个理论".into(),
            "主角从外星信号里读出了关键信息".into(),
            "敌对方的内应其实是主角当年的老师".into(),
        ],
    }
}

fn fantasy_template() -> GenreTemplate {
    GenreTemplate {
        genre: Genre::Fantasy,
        display_name: "西幻 / 奇幻".into(),
        pace: PaceTarget {
            recommended_words: 2200,
            min_words: 1800,
            max_words: 2800,
        },
        scene_mix: SceneMix {
            dialogue: 30,
            action: 30,
            description: 30,
            introspection: 10,
        },
        arc: ArcTemplate {
            total_chapters: 50,
            phases: xianxia_template().arc.phases,
        },
        key_principles: vec![
            "种族 / 教派 / 血脉体系要设定清晰".into(),
            "魔法代价要明确，否则设定会显得廉价".into(),
            "反派最好有自己的信念，不要单纯「邪恶」".into(),
        ],
        dopamine_moments: vec![
            "古龙苏醒 / 神器认主".into(),
            "屠龙小队全员内鬼".into(),
            "主角用契约反噬了恶魔领主".into(),
        ],
    }
}

fn historical_template() -> GenreTemplate {
    GenreTemplate {
        genre: Genre::Historical,
        display_name: "历史 / 穿越".into(),
        pace: PaceTarget {
            recommended_words: 2200,
            min_words: 1800,
            max_words: 2800,
        },
        scene_mix: SceneMix {
            dialogue: 35,
            action: 20,
            description: 30,
            introspection: 15,
        },
        arc: ArcTemplate {
            total_chapters: 50,
            phases: vec![
                ArcPhase {
                    name: "立人设".into(),
                    goal: "交代时代背景 / 主角身份 / 核心矛盾".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "站稳脚跟".into(),
                    goal: "用现代知识获取第一波资源".into(),
                    weight: 0.3,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "卷入大事件".into(),
                    goal: "被历史浪潮裹挟".into(),
                    weight: 0.3,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "改变历史".into(),
                    goal: "在关键节点做出选择".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
            ],
        },
        key_principles: vec![
            "时代术语 / 官职 / 礼仪要符合史实（架空也要自洽）".into(),
            "现代知识不可「超出时代太多」地解决问题".into(),
            "感情线要慢，符合古代节奏".into(),
        ],
        dopamine_moments: vec![
            "诗词 / 计策惊艳全场".into(),
            "被怀疑是妖邪的现代主角反手破局".into(),
            "皇帝私下接见".into(),
        ],
    }
}

fn romance_template() -> GenreTemplate {
    GenreTemplate {
        genre: Genre::Romance,
        display_name: "言情".into(),
        pace: PaceTarget {
            recommended_words: 1500,
            min_words: 1200,
            max_words: 2200,
        },
        scene_mix: SceneMix {
            dialogue: 50,
            action: 10,
            description: 20,
            introspection: 20,
        },
        arc: ArcTemplate {
            total_chapters: 40,
            phases: vec![
                ArcPhase {
                    name: "初遇".into(),
                    goal: "男女主相遇，建立张力".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "试探".into(),
                    goal: "关系推进，反复拉扯".into(),
                    weight: 0.4,
                    requires_hook: false,
                },
                ArcPhase {
                    name: "误会 / 拆伙".into(),
                    goal: "出现第三方 / 旧情人搅局".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "复合 / 抉择".into(),
                    goal: "真相大白，关系明确".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
            ],
        },
        key_principles: vec![
            "情感要落到具体事件，不能只是心理描写".into(),
            "误会必须有充分动机，不要强行误会".into(),
            "配角要立体，避免工具人化".into(),
        ],
        dopamine_moments: vec![
            "久别重逢 / 雨中相拥".into(),
            "高甜公开撒糖".into(),
            "曾经的对手成了最懂自己的人".into(),
        ],
    }
}

fn suspense_template() -> GenreTemplate {
    GenreTemplate {
        genre: Genre::Suspense,
        display_name: "悬疑 / 推理".into(),
        pace: PaceTarget {
            recommended_words: 2200,
            min_words: 1800,
            max_words: 2800,
        },
        scene_mix: SceneMix {
            dialogue: 35,
            action: 15,
            description: 30,
            introspection: 20,
        },
        arc: ArcTemplate {
            total_chapters: 45,
            phases: vec![
                ArcPhase {
                    name: "事件发生".into(),
                    goal: "案件 / 异常发生，主角介入".into(),
                    weight: 0.15,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "线索堆积".into(),
                    goal: "看似无关的线索接连出现".into(),
                    weight: 0.45,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "迷雾加深".into(),
                    goal: "看似嫌疑人逐一被排除".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "真相 + 反转".into(),
                    goal: "真相大白，并附一个意料之外的第二层真相".into(),
                    weight: 0.2,
                    requires_hook: true,
                },
            ],
        },
        key_principles: vec![
            "线索要在前文就有伏笔，不要临时抛出".into(),
            "证据链要可验证，不要靠巧合破案".into(),
            "每章结尾必须留一个钩子".into(),
        ],
        dopamine_moments: vec![
            "看似无关的细节突然串成完整链条".into(),
            "「凶手」居然是主角最亲近的人".into(),
            "主角用凶手才知道的信息设下陷阱".into(),
        ],
    }
}

fn other_template() -> GenreTemplate {
    GenreTemplate {
        genre: Genre::Other,
        display_name: "其他".into(),
        pace: PaceTarget {
            recommended_words: 2000,
            min_words: 1500,
            max_words: 2800,
        },
        scene_mix: SceneMix {
            dialogue: 35,
            action: 25,
            description: 25,
            introspection: 15,
        },
        arc: ArcTemplate {
            total_chapters: 40,
            phases: vec![
                ArcPhase {
                    name: "开篇".into(),
                    goal: "立人设，交代背景".into(),
                    weight: 0.25,
                    requires_hook: true,
                },
                ArcPhase {
                    name: "推进".into(),
                    goal: "事件层层推进".into(),
                    weight: 0.5,
                    requires_hook: false,
                },
                ArcPhase {
                    name: "高潮".into(),
                    goal: "卷末冲突爆发".into(),
                    weight: 0.25,
                    requires_hook: true,
                },
            ],
        },
        key_principles: vec!["先把核心冲突讲清楚；节奏均匀，钩子稳定".into()],
        dopamine_moments: vec!["主角第一次真正意义上的胜利".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_for_chapter_returns_correct_phase() {
        let tpl = GenreTemplate::for_genre(Genre::Xianxia);
        // 50 章：0-10 开篇；10-25 小试牛刀；25-40 中段反转；40-50 卷末高潮
        assert_eq!(tpl.arc.phase_for_chapter(1).unwrap().name, "开篇立人设");
        assert_eq!(tpl.arc.phase_for_chapter(20).unwrap().name, "小试牛刀");
        assert_eq!(tpl.arc.phase_for_chapter(45).unwrap().name, "卷末高潮");
    }

    #[test]
    fn all_genres_have_template() {
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
            let t = GenreTemplate::for_genre(g.clone());
            assert!(!t.key_principles.is_empty());
            assert!(t.pace.recommended_words > 0);
            assert!(t.arc.total_chapters > 0);
        }
    }
}

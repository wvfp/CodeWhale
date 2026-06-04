//! 子 Agent 流水线。
//!
//! `Pipeline` 定义了从「用户一句话灵感」到「可发布的章节」的标准 5 阶段流：
//!
//! ```text
//! ArchitectConcept
//!   → OutlinerArc
//!   → OutlinerChapter
//!   → WriterDraft
//!   → EditorPolish
//! ```
//!
//! 实际运行由 LLM 驱动（每个步骤通过 system prompt + user prompt 交给模型）。
//! 本模块提供「步骤是什么 / 每个步骤要给模型什么提示 / 每个步骤要存到哪」
//! 的可枚举定义。

use serde::{Deserialize, Serialize};

use super::agents::AgentRole;
use super::state::PipelinePhase;
use crate::model::project::Genre;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStep {
    ArchitectConcept,
    OutlinerArc,
    OutlinerChapter,
    WriterDraft,
    EditorPolish,
}

impl PipelineStep {
    pub fn label(self) -> &'static str {
        match self {
            PipelineStep::ArchitectConcept => "架构师：搭骨架",
            PipelineStep::OutlinerArc => "大纲师：分卷",
            PipelineStep::OutlinerChapter => "大纲师：分章",
            PipelineStep::WriterDraft => "写手：写正文",
            PipelineStep::EditorPolish => "编辑：审校",
        }
    }

    pub fn next(self) -> Option<Self> {
        match self {
            PipelineStep::ArchitectConcept => Some(PipelineStep::OutlinerArc),
            PipelineStep::OutlinerArc => Some(PipelineStep::OutlinerChapter),
            PipelineStep::OutlinerChapter => Some(PipelineStep::WriterDraft),
            PipelineStep::WriterDraft => Some(PipelineStep::EditorPolish),
            PipelineStep::EditorPolish => None,
        }
    }

    pub fn agent(self) -> AgentRole {
        match self {
            PipelineStep::ArchitectConcept => AgentRole::Architect,
            PipelineStep::OutlinerArc | PipelineStep::OutlinerChapter => AgentRole::Outliner,
            PipelineStep::WriterDraft => AgentRole::Writer,
            PipelineStep::EditorPolish => AgentRole::Editor,
        }
    }

    pub fn phase(self) -> PipelinePhase {
        match self {
            PipelineStep::ArchitectConcept => PipelinePhase::Concept,
            PipelineStep::OutlinerArc | PipelineStep::OutlinerChapter => PipelinePhase::Outline,
            PipelineStep::WriterDraft => PipelinePhase::Draft,
            PipelineStep::EditorPolish => PipelinePhase::Polish,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineInput {
    /// 用户的原始灵感（一句话 / 几行 / 关键词）。
    pub idea: String,
    /// 题材。
    pub genre: Genre,
    /// 目标章节数（默认 0 = 由大纲师决定）。
    pub target_chapters: u32,
    /// 当前要写的章节号（draft 阶段必填）。
    pub chapter: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineResult {
    pub step: PipelineStep,
    /// 步骤产出（人话或 JSON）。
    pub output: String,
    /// 写出去的工件（文件路径等）。
    pub artifacts: Vec<String>,
}

pub struct Pipeline {
    pub genre: Genre,
}

impl Pipeline {
    pub fn new(genre: Genre) -> Self {
        Self { genre }
    }

    /// 返回一次完整流程要走的步骤列表。
    pub fn full_flow() -> Vec<PipelineStep> {
        vec![
            PipelineStep::ArchitectConcept,
            PipelineStep::OutlinerArc,
            PipelineStep::OutlinerChapter,
            PipelineStep::WriterDraft,
            PipelineStep::EditorPolish,
        ]
    }

    /// 返回只跑「写手 + 编辑」两步的快速流程。
    pub fn draft_only_flow() -> Vec<PipelineStep> {
        vec![PipelineStep::WriterDraft, PipelineStep::EditorPolish]
    }

    /// 计算下一步的步骤。
    pub fn next_step(step: PipelineStep) -> Option<PipelineStep> {
        step.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_sequence_is_well_formed() {
        let flow = Pipeline::full_flow();
        assert_eq!(flow.len(), 5);
        for w in flow.windows(2) {
            assert_eq!(w[0].next(), Some(w[1]));
        }
        assert_eq!(flow.last().unwrap().next(), None);
    }

    #[test]
    fn every_step_maps_to_agent_and_phase() {
        for s in [
            PipelineStep::ArchitectConcept,
            PipelineStep::OutlinerArc,
            PipelineStep::OutlinerChapter,
            PipelineStep::WriterDraft,
            PipelineStep::EditorPolish,
        ] {
            let _ = s.agent();
            let _ = s.phase();
        }
    }
}

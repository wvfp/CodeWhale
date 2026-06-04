//! 流水线状态：把「这章已经走到哪一步 / 上一步的产物」持久化到磁盘。

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::agents::AgentRole;
use super::pipeline::PipelineStep;
use crate::model::project::Genre;

/// 流水线状态文件路径。
pub const PIPELINE_PATH: &str = ".novelwhale/pipeline.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PipelinePhase {
    Concept,
    Outline,
    Detail,
    Draft,
    Polish,
}

impl PipelinePhase {
    pub fn label(self) -> &'static str {
        match self {
            PipelinePhase::Concept => "概念",
            PipelinePhase::Outline => "大纲",
            PipelinePhase::Detail => "细纲",
            PipelinePhase::Draft => "正文",
            PipelinePhase::Polish => "润色",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    Pending,
    Running,
    Done,
    Failed,
    NeedsHumanInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineRecord {
    /// 当前章节号（如果是大纲阶段则不写）。
    pub chapter: Option<u32>,
    /// 题材。
    pub genre: Genre,
    /// 当前阶段。
    pub phase: PipelinePhase,
    /// 当前步骤。
    pub step: PipelineStep,
    /// 状态。
    pub status: PipelineStatus,
    /// 当前负责的 Agent。
    pub current_agent: AgentRole,
    /// 已完成的步骤列表。
    pub completed_steps: Vec<PipelineStep>,
    /// 上一步产物的简短描述（人话）。
    pub last_artifact_summary: String,
    /// 更新时间（RFC3339）。
    pub updated_at: String,
}

impl PipelineRecord {
    pub fn new(genre: Genre) -> Self {
        Self {
            chapter: None,
            genre,
            phase: PipelinePhase::Concept,
            step: PipelineStep::ArchitectConcept,
            status: PipelineStatus::Pending,
            current_agent: AgentRole::Architect,
            completed_steps: Vec::new(),
            last_artifact_summary: String::new(),
            updated_at: now_iso8601(),
        }
    }

    pub fn load(project_root: &Path) -> crate::rag::types::RagResult<Self> {
        let path = project_root.join(PIPELINE_PATH);
        if !path.exists() {
            return Err(crate::rag::types::RagError::Retriever(
                "pipeline state not found".into(),
            ));
        }
        let raw = std::fs::read_to_string(&path)?;
        let rec: Self = serde_json::from_str(&raw).unwrap_or_else(|_| Self::new(Genre::Other));
        Ok(rec)
    }

    pub fn save(&self, project_root: &Path) -> crate::rag::types::RagResult<()> {
        let path = project_root.join(PIPELINE_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pretty = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, pretty)?;
        Ok(())
    }
}

pub(crate) fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch:{}", secs)
}

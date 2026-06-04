//! Novel writing tools.
//!
//! These tools implement the novel creation features added by the
//! CodeWhale → NovelWhale transformation. They are designed to be
//! registered into the standard `ToolRegistry` via the
//! [`register_novel_tools`] function.
//!
//! # Available tools
//!
//! | Tool                | Purpose                                                |
//! |---------------------|--------------------------------------------------------|
//! | `novel_init`        | Initialize a novel project with title, genre, platform |
//! | `novel_outline_build` | Build a narrative outline for an arc               |
//! | `novel_write`       | Draft a chapter following an outline                   |
//! | `novel_fact_lock`   | Lock a fact as immutable (cannot be contradicted)      |
//! | `novel_read_state`  | Read the current novel session state                   |
//! | `novel_stage_set`   | Switch the active creation stage                       |
//! | `novel_synopsis_read` | Read the chapter synopsis for context compression     |
//! | `novel_consistency_check` | Run a consistency audit on a single chapter       |
//! | `novel_rag_search`    | RAG search over facts / characters / synopses             |
//! | `novel_deai_check`    | De-AI audit (cliché scan) on a single chapter             |
//! | `novel_genre_template` | Look up the template (arc / pace / rules) for a genre    |
//! | `novel_pipeline_run`    | Start / advance a sub-agent pipeline (Architect → Editor) |
//! | `novel_pipeline_advance` | Mark current pipeline step done and move to next       |

pub mod branch_explore;
pub mod consistency_check;
pub mod deai_check;
pub mod fact_lock;
pub mod genre_template;
pub mod init;
pub mod outline_build;
pub mod pipeline_run;
pub mod rag_search;
pub mod reader_simulate;
pub mod read_state;
pub mod slump_diagnose;
pub mod spec;
pub mod stage_set;
pub mod synopsis_read;
pub mod write;

pub use spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_str, optional_u64, required_str,
};

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

/// Error type for novel tool operations.
#[derive(Debug, Error)]
pub enum NovelToolError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("project not initialized: run `novel_init` first")]
    NotInitialized,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl From<NovelToolError> for ToolError {
    fn from(e: NovelToolError) -> Self {
        ToolError::execution_failed(e.to_string())
    }
}

/// Register all novel writing tools into the provided tool list.
pub fn register_novel_tools(tools: &mut Vec<Arc<dyn ToolSpec>>) {
    tools.push(Arc::new(init::NovelInitTool));
    tools.push(Arc::new(outline_build::NovelOutlineBuildTool));
    tools.push(Arc::new(write::NovelWriteTool));
    tools.push(Arc::new(fact_lock::NovelFactLockTool));
    tools.push(Arc::new(read_state::NovelReadStateTool));
    tools.push(Arc::new(stage_set::NovelStageSetTool));
    tools.push(Arc::new(synopsis_read::NovelSynopsisReadTool));
    tools.push(Arc::new(consistency_check::NovelConsistencyCheckTool));
    tools.push(Arc::new(rag_search::NovelRagSearchTool));
    tools.push(Arc::new(deai_check::NovelDeaiCheckTool));
    tools.push(Arc::new(branch_explore::NovelBranchExploreTool));
    tools.push(Arc::new(slump_diagnose::NovelSlumpDiagnoseTool));
    tools.push(Arc::new(reader_simulate::NovelReaderSimulateTool));
}

/// Trait shared by all novel tools to support a uniform `name()` impl.
#[async_trait]
pub trait NovelTool: ToolSpec + Send + Sync {}

/// Required capability marker for tools that write into the workspace.
#[allow(dead_code)]
pub(crate) fn write_capabilities() -> Vec<ToolCapability> {
    vec![ToolCapability::WritesFiles, ToolCapability::Sandboxable]
}

/// Required capability marker for read-only tools.
pub(crate) fn readonly_capabilities() -> Vec<ToolCapability> {
    vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
}

/// Common approval: most novel tools need explicit user consent.
#[allow(dead_code)]
pub(crate) fn suggest_approval() -> ApprovalRequirement {
    ApprovalRequirement::Suggest
}

//! Adapter that wraps `novel::tools::spec::ToolSpec` implementations so
//! they can live inside the TUI's [`ToolRegistry`].
//!
//! The novel crate can't depend on the TUI crate (that would be a cycle
//! once the TUI depends back on `novel`), so the novel tools implement
//! a slim in-crate `ToolSpec` trait. Each invocation of a novel tool at
//! runtime needs:
//!
//! - to expose the novel tool's `name` / `description` / `input_schema` /
//!   `capabilities` to the TUI's tool catalog
//! - to forward `execute` calls, downgrading the TUI's `ToolContext` to
//!   the minimal subset the novel trait expects and upgrading the
//!   novel `ToolResult` back to the TUI's richer `ToolResult` type.
//!
//! The mapping here is intentionally lossy — the novel `ToolContext`
//! only carries `workspace`, `trust_mode`, `notes_path`, `auto_approve`,
//! and a cancellation token. We don't bridge every TUI service into the
//! novel world; the novel tools operate against a workspace directory
//! and the OS file system, and that's all they need to do their job.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::spec::{ToolCapability as TuiToolCapability, ToolContext, ToolError, ToolResult};
use novel::tools::spec::{
    ApprovalRequirement as NovelApprovalRequirement, ToolCapability as NovelToolCapability,
    ToolContext as NovelToolContext, ToolResult as NovelToolResult, ToolSpec as NovelToolSpec,
};

/// Wrap a novel tool so it implements the TUI's `ToolSpec` trait.
pub(crate) struct NovelAdapter {
    inner: Arc<dyn NovelToolSpec>,
}

impl NovelAdapter {
    pub(crate) fn new(inner: Arc<dyn NovelToolSpec>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl super::spec::ToolSpec for NovelAdapter {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn input_schema(&self) -> Value {
        self.inner.input_schema()
    }

    fn capabilities(&self) -> Vec<TuiToolCapability> {
        self.inner.capabilities().into_iter().map(convert_capability).collect()
    }

    fn approval_requirement(&self) -> super::spec::ApprovalRequirement {
        match self.inner.approval_requirement() {
            NovelApprovalRequirement::Auto => super::spec::ApprovalRequirement::Auto,
            NovelApprovalRequirement::Suggest => super::spec::ApprovalRequirement::Suggest,
            NovelApprovalRequirement::Required => super::spec::ApprovalRequirement::Required,
        }
    }

    fn supports_parallel(&self) -> bool {
        self.inner.supports_parallel()
    }

    fn defer_loading(&self) -> bool {
        // Novel tools are the primary surface of a Novel-mode session, so
        // keep them in the model-visible catalog instead of deferring them.
        false
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let novel_ctx = NovelToolContext {
            workspace: context.workspace.clone(),
            trust_mode: context.trust_mode,
            notes_path: context.notes_path.clone(),
            auto_approve: context.auto_approve,
            cancellation_token: None,
        };

        let novel_result: NovelToolResult = self
            .inner
            .execute(input, &novel_ctx)
            .await
            .map_err(convert_tool_error)?;

        Ok(ToolResult {
            content: novel_result.content,
            success: novel_result.success,
            metadata: novel_result.metadata,
        })
    }
}

fn convert_tool_error(err: novel::tools::spec::ToolError) -> ToolError {
    // The novel crate's `ToolError` is a deliberately small variant set; the
    // TUI's `ToolError` (re-exported from `codewhale_tools`) is the canonical
    // type. Map the variant tags and stringify any sub-fields so the TUI's
    // user-facing error rendering continues to work.
    use novel::tools::spec::ToolError as NovelErr;
    match err {
        NovelErr::InvalidInput { message } => ToolError::InvalidInput { message },
        NovelErr::MissingField { field } => ToolError::MissingField { field },
        NovelErr::PathEscape { path } => ToolError::PathEscape { path },
        NovelErr::ExecutionFailed { message } => ToolError::ExecutionFailed { message },
        NovelErr::Timeout { seconds } => ToolError::Timeout { seconds },
        NovelErr::NotAvailable { message } => ToolError::NotAvailable { message },
        NovelErr::PermissionDenied { message } => ToolError::PermissionDenied { message },
    }
}

fn convert_capability(cap: NovelToolCapability) -> TuiToolCapability {
    match cap {
        NovelToolCapability::ReadOnly => TuiToolCapability::ReadOnly,
        NovelToolCapability::WritesFiles => TuiToolCapability::WritesFiles,
        NovelToolCapability::ExecutesCode => TuiToolCapability::ExecutesCode,
        NovelToolCapability::Network => TuiToolCapability::Network,
        NovelToolCapability::Sandboxable => TuiToolCapability::Sandboxable,
        NovelToolCapability::RequiresApproval => TuiToolCapability::RequiresApproval,
    }
}

/// Build a list of `Arc<dyn TUI ToolSpec>` from the novel crate's
/// `register_novel_tools` callback. Each novel tool is wrapped in a
/// `NovelAdapter` so it can be stored alongside native TUI tools in
/// the unified `ToolRegistry`.
pub(crate) fn collect_novel_tools() -> Vec<Arc<dyn super::spec::ToolSpec>> {
    let mut novel_tools: Vec<Arc<dyn NovelToolSpec>> = Vec::new();
    novel::tools::register_novel_tools(&mut novel_tools);
    novel_tools
        .into_iter()
        .map(|t| Arc::new(NovelAdapter::new(t)) as Arc<dyn super::spec::ToolSpec>)
        .collect()
}

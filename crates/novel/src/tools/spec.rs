//! Minimal `ToolSpec` trait used by novel tools.
//!
//! The novel crate can't depend on the TUI crate (that would be a cycle
//! once the TUI depends back on `novel`), so we re-declare a slim subset
//! of the TUI's `ToolSpec` API here. The TUI integrates these tools via
//! thin adapter wrappers (`crates/tui/src/tools/novel_adapter.rs`).
//!
//! Keep the trait surface small and stable — anything added here is
//! effectively part of the novel crate's public API.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Simple cancellation token used by novel tools. Mirrors a minimal subset of
/// `tokio_util::sync::CancellationToken` so the novel crate stays free of
/// `tokio-util` (which isn't a workspace dependency).
#[derive(Debug, Default, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Capabilities a tool may have or require. Mirrors the TUI's enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolCapability {
    /// Read-only operation.
    ReadOnly,
    /// Writes to the filesystem.
    WritesFiles,
    /// Executes arbitrary code/commands.
    ExecutesCode,
    /// Makes network requests.
    Network,
    /// Can run inside the workspace sandbox.
    Sandboxable,
    /// Requires user approval before execution.
    RequiresApproval,
}

/// Approval requirement for a tool. Mirrors the TUI's enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ApprovalRequirement {
    /// Auto-approve: safe read-only operations.
    #[default]
    Auto,
    /// Suggest approval; user can skip.
    Suggest,
    /// Always require explicit approval.
    Required,
}

/// Errors that can occur during tool execution. Mirrors the TUI's enum.
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum ToolError {
    #[error("Failed to validate input: {message}")]
    InvalidInput { message: String },
    #[error("Failed to validate input: missing required field '{field}'")]
    MissingField { field: String },
    #[error("Failed to resolve path '{path}': path escapes workspace", path = .path.display())]
    PathEscape { path: PathBuf },
    #[error("Failed to execute tool: {message}")]
    ExecutionFailed { message: String },
    #[error("Failed to execute tool: operation timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("Failed to locate tool: {message}")]
    NotAvailable { message: String },
    #[error("Failed to authorize tool execution: {message}")]
    PermissionDenied { message: String },
}

impl ToolError {
    #[must_use]
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: msg.into(),
        }
    }

    #[must_use]
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField {
            field: field.into(),
        }
    }

    #[must_use]
    pub fn execution_failed(msg: impl Into<String>) -> Self {
        Self::ExecutionFailed {
            message: msg.into(),
        }
    }
}

/// Result of a tool execution. Mirrors the TUI's struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Output content (JSON or plain text).
    pub content: String,
    /// Whether the execution succeeded.
    pub success: bool,
    /// Optional structured metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl ToolResult {
    #[must_use]
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            success: true,
            metadata: None,
        }
    }

    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: message.into(),
            success: false,
            metadata: None,
        }
    }

    pub fn json<T: Serialize>(value: &T) -> std::result::Result<Self, serde_json::Error> {
        Ok(Self {
            content: serde_json::to_string_pretty(value)?,
            success: true,
            metadata: None,
        })
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Context passed to tools during execution. Mirrors the TUI's struct.
#[derive(Clone)]
pub struct ToolContext {
    /// The workspace root directory.
    pub workspace: PathBuf,
    /// Whether to allow paths outside the workspace.
    #[allow(dead_code)]
    pub trust_mode: bool,
    /// Path for the notes file.
    #[allow(dead_code)]
    pub notes_path: PathBuf,
    /// Whether tools should auto-approve without safety checks.
    #[allow(dead_code)]
    pub auto_approve: bool,
    /// Cancellation token for cooperative cancellation.
    #[allow(dead_code)]
    pub cancellation_token: Option<CancellationToken>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("workspace", &self.workspace)
            .field("trust_mode", &self.trust_mode)
            .field("notes_path", &self.notes_path)
            .field("auto_approve", &self.auto_approve)
            .field("cancellation_token", &self.cancellation_token.is_some())
            .finish()
    }
}

impl ToolContext {
    /// Build a minimal context scoped to a workspace directory.
    #[must_use]
    pub fn for_workspace(workspace: impl Into<PathBuf>) -> Self {
        let workspace = workspace.into();
        Self {
            notes_path: workspace.join(".novelwhale").join("notes.md"),
            workspace,
            trust_mode: false,
            auto_approve: false,
            cancellation_token: None,
        }
    }
}

/// The trait that all novel tools implement.
///
/// This is a minimal subset of the TUI's `ToolSpec` so that the novel
/// crate can be a leaf dependency. The TUI's adapter wraps these
/// implementations to plug them into its own registry.
#[async_trait]
pub trait ToolSpec: Send + Sync {
    /// The tool's name; must be unique within a registry.
    fn name(&self) -> &'static str;

    /// Human-readable description shown to the model.
    fn description(&self) -> &'static str;

    /// JSON Schema describing the input.
    fn input_schema(&self) -> Value;

    /// The tool's capabilities (used for capability-based filtering).
    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Sandboxable]
    }

    /// Whether the tool supports concurrent invocations.
    fn supports_parallel(&self) -> bool {
        true
    }

    /// Optional per-call timeout.
    fn timeout(&self) -> Option<Duration> {
        None
    }

    /// Approval requirement override (defaults to `Auto`).
    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    /// Execute the tool with the provided input and context.
    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError>;
}

// === Helper extractors (mirrors of codewhale-tools helpers) ===

/// Extract a required string field.
pub fn required_str<'a>(input: &'a Value, field: &str) -> std::result::Result<&'a str, ToolError> {
    input.get(field).and_then(Value::as_str).ok_or_else(|| {
        let provided: Vec<&str> = input
            .as_object()
            .map(|obj| obj.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        if provided.is_empty() {
            ToolError::missing_field(field)
        } else {
            let hint = format!(
                "missing required field '{field}'. Input provided: {}",
                provided.join(", ")
            );
            ToolError::invalid_input(hint)
        }
    })
}

/// Extract an optional string field.
#[must_use]
pub fn optional_str<'a>(input: &'a Value, field: &str) -> Option<&'a str> {
    input.get(field).and_then(Value::as_str)
}

/// Extract an optional u64 field with default.
#[must_use]
pub fn optional_u64(input: &Value, field: &str, default: u64) -> u64 {
    input.get(field).and_then(Value::as_u64).unwrap_or(default)
}

/// Extract an optional bool field with default.
#[must_use]
pub fn optional_bool(input: &Value, field: &str, default: bool) -> bool {
    input.get(field).and_then(Value::as_bool).unwrap_or(default)
}

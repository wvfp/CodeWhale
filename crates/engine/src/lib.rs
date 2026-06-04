//! UI-agnostic engine abstraction for CodeWhale multi-frontend architecture.
//!
//! This crate defines the serializable operation and event types that flow
//! between a frontend (TUI, Web, Android) and the core engine. It has **no**
//! dependency on any UI framework (ratatui, crossterm, schemaui, etc.) so it
//! can be shared across all frontends.

pub mod engine_api;
pub mod events;
pub mod handle;
pub mod ops;

// Re-export the key types at the crate root for convenience.
pub use engine_api::{
    ApprovalDecision, CancelReason, Engine, SandboxPolicyPayload,
    UserInputAnswerPayload, UserInputDecision, UserInputResponsePayload,
};
pub use events::EngineEvent;
pub use handle::EngineHandle;
pub use ops::{CompactionConfigPayload, EngineOp};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AppMode
// ---------------------------------------------------------------------------

/// Supported application modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppMode {
    Agent,
    Yolo,
    Plan,
    /// Long-form web-novel writing mode.
    Novel,
}

impl AppMode {
    /// Parse a mode from a settings/config string value.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "plan" => Self::Plan,
            "yolo" => Self::Yolo,
            "novel" => Self::Novel,
            _ => Self::Agent,
        }
    }

    /// Serialize to a settings/config string value.
    #[must_use]
    pub fn as_setting(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Yolo => "yolo",
            Self::Plan => "plan",
            Self::Novel => "novel",
        }
    }

    /// Short label used in the UI footer.
    pub fn label(self) -> &'static str {
        match self {
            AppMode::Agent => "AGENT",
            AppMode::Yolo => "YOLO",
            AppMode::Plan => "PLAN",
            AppMode::Novel => "NOVEL",
        }
    }

    /// Description shown in help or onboarding text.
    #[allow(dead_code)]
    pub fn description(self) -> &'static str {
        match self {
            AppMode::Agent => "Agent mode - autonomous task execution with tools",
            AppMode::Yolo => "YOLO mode - full tool access without approvals",
            AppMode::Plan => "Plan mode - design before implementing",
            AppMode::Novel => "Novel mode - long-form web novel writing assistant",
        }
    }
}

// ---------------------------------------------------------------------------
// ApprovalMode
// ---------------------------------------------------------------------------

/// Determines when tool executions require user approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// Auto-approve all tools (YOLO mode / --yolo flag).
    Auto,
    /// Suggest approval for non-safe tools (non-YOLO modes).
    #[default]
    Suggest,
    /// Never execute tools requiring approval.
    Never,
}

impl ApprovalMode {
    pub fn label(self) -> &'static str {
        match self {
            ApprovalMode::Auto => "AUTO",
            ApprovalMode::Suggest => "SUGGEST",
            ApprovalMode::Never => "NEVER",
        }
    }

    pub fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(ApprovalMode::Auto),
            "suggest" | "suggested" | "on-request" | "untrusted" => Some(ApprovalMode::Suggest),
            "never" | "deny" | "denied" => Some(ApprovalMode::Never),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ToolErrorPayload
// ---------------------------------------------------------------------------

/// Serializable representation of a tool error.
///
/// Unlike `codewhale_tools::ToolError` (which uses `thiserror` and is not
/// serializable), this type can be sent across process boundaries to
/// Web/Android frontends.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolErrorPayload {
    InvalidInput { message: String },
    MissingField { field: String },
    PathEscape { path: String },
    ExecutionFailed { message: String },
    Timeout { seconds: u64 },
    NotAvailable { message: String },
    PermissionDenied { message: String },
}

impl From<&codewhale_tools::ToolError> for ToolErrorPayload {
    fn from(err: &codewhale_tools::ToolError) -> Self {
        match err {
            codewhale_tools::ToolError::InvalidInput { message } => {
                ToolErrorPayload::InvalidInput { message: message.clone() }
            }
            codewhale_tools::ToolError::MissingField { field } => {
                ToolErrorPayload::MissingField { field: field.clone() }
            }
            codewhale_tools::ToolError::PathEscape { path } => {
                ToolErrorPayload::PathEscape { path: path.to_string_lossy().to_string() }
            }
            codewhale_tools::ToolError::ExecutionFailed { message } => {
                ToolErrorPayload::ExecutionFailed { message: message.clone() }
            }
            codewhale_tools::ToolError::Timeout { seconds } => {
                ToolErrorPayload::Timeout { seconds: *seconds }
            }
            codewhale_tools::ToolError::NotAvailable { message } => {
                ToolErrorPayload::NotAvailable { message: message.clone() }
            }
            codewhale_tools::ToolError::PermissionDenied { message } => {
                ToolErrorPayload::PermissionDenied { message: message.clone() }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TurnOutcomeStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnOutcomeStatus {
    Completed,
    Interrupted,
    Failed,
}

// ---------------------------------------------------------------------------
// NotificationKind
// ---------------------------------------------------------------------------

/// Kinds of notifications the engine can emit.
///
/// Replaces direct calls to `tui::notifications` so the engine does not
/// depend on any UI framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    TaskbarBusy,
    TaskbarIdle,
    TitleAnimationStart,
    TitleAnimationStop,
}

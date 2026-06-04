//! UI-agnostic engine trait and serializable decision types.
//!
//! All frontends (TUI, Web, GUI, Android) interact with the engine through
//! the [`Engine`] trait. The trait is async and uses only serializable types
//! so it can be driven over a process boundary if needed.

use std::path::PathBuf;

use async_trait::async_trait;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::ops::EngineOp;
use crate::events::EngineEvent;

// ---------------------------------------------------------------------------
// CancelReason
// ---------------------------------------------------------------------------

/// Reason the active turn was cancelled. The token from `tokio_util`
/// does not carry a cause, so the engine keeps a sibling latch for
/// approval and user-input waits that need to explain cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    /// User-initiated cancel (Esc, `/cancel`, click cancel on modal).
    User,
    /// External / runtime-API cancel (HTTP `DELETE /v1/threads/…`,
    /// task manager stop, parent agent cancel).
    External,
    /// Cancel triggered when a new turn starts before the previous one
    /// finished — e.g. plain Enter while busy after the queueing path
    /// pre-empts the running turn.
    Preempted,
    /// Engine internals tore down the turn (drop, channel close,
    /// shutdown). Rare — surfaced as an internal error.
    Internal,
}

impl CancelReason {
    /// Human-readable description of the cancel reason.
    pub fn describe(self) -> &'static str {
        match self {
            Self::User => "user cancelled the request",
            Self::External => "request cancelled by external caller",
            Self::Preempted => "request was preempted by a new turn",
            Self::Internal => "engine torn down before approval resolved",
        }
    }
}

// ---------------------------------------------------------------------------
// SandboxPolicyPayload
// ---------------------------------------------------------------------------

/// Serializable representation of a sandbox policy.
///
/// Mirrors `tui::sandbox::SandboxPolicy` but lives in the engine crate
/// so it does not pull in any UI dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SandboxPolicyPayload {
    /// No restrictions whatsoever.
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,

    /// Read-only access to the entire filesystem.
    #[serde(rename = "read-only")]
    ReadOnly,

    /// Already running in an external sandbox.
    #[serde(rename = "external-sandbox")]
    ExternalSandbox {
        #[serde(default)]
        network_access: bool,
    },

    /// Read-only filesystem plus write access to specified directories.
    #[serde(rename = "workspace-write")]
    WorkspaceWrite {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writable_roots: Vec<PathBuf>,
        #[serde(default)]
        network_access: bool,
        #[serde(default)]
        exclude_tmpdir: bool,
        #[serde(default)]
        exclude_slash_tmp: bool,
    },
}

// ---------------------------------------------------------------------------
// ApprovalDecision
// ---------------------------------------------------------------------------

/// Decision submitted by the frontend when a tool call requires approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ApprovalDecision {
    /// Approve the tool execution.
    Approved { id: String },
    /// Deny the tool execution.
    Denied { id: String },
    /// Retry with an elevated sandbox policy.
    RetryWithPolicy { id: String, policy: SandboxPolicyPayload },
}

// ---------------------------------------------------------------------------
// UserInputDecision
// ---------------------------------------------------------------------------

/// Decision submitted by the frontend when a tool requests user input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum UserInputDecision {
    /// User submitted answers to the elicitation questions.
    Submitted {
        id: String,
        response: UserInputResponsePayload,
    },
    /// User cancelled the elicitation.
    Cancelled { id: String },
}

// ---------------------------------------------------------------------------
// UserInputResponsePayload
// ---------------------------------------------------------------------------

/// Serializable representation of a user-input response.
///
/// Mirrors `tui::tools::user_input::UserInputResponse` but lives in the
/// engine crate so it does not pull in any UI dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputResponsePayload {
    pub answers: Vec<UserInputAnswerPayload>,
}

/// Serializable representation of a single user-input answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputAnswerPayload {
    pub id: String,
    pub label: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// Engine trait
// ---------------------------------------------------------------------------

/// UI-agnostic engine interface. All frontends (TUI, Web, GUI, Android)
/// interact with the engine through this trait.
#[async_trait]
pub trait Engine: Send + Sync {
    /// Send an operation to the engine.
    async fn send_op(&self, op: EngineOp) -> Result<()>;

    /// Try to receive an event from the engine (non-blocking).
    /// Returns `None` if no event is available.
    async fn recv_event(&self) -> Option<EngineEvent>;

    /// Submit an approval decision for a pending tool call.
    async fn submit_approval(&self, decision: ApprovalDecision) -> Result<()>;

    /// Submit user input for a pending elicitation.
    async fn submit_user_input(&self, response: UserInputDecision) -> Result<()>;

    /// Cancel the current operation.
    async fn cancel(&self, reason: Option<String>) -> Result<()>;

    /// Gracefully shutdown the engine.
    async fn shutdown(&self) -> Result<()>;
}

//! Events emitted by the core engine to any frontend.
//!
//! These events flow from the engine to the UI (TUI, Web, Android) via a
//! channel, enabling non-blocking, real-time updates. All types are fully
//! serializable so they can cross process boundaries.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{NotificationKind, ToolErrorPayload, TurnOutcomeStatus};
use codewhale_tools::ToolResult;

// ---------------------------------------------------------------------------
// Serializable payload types for tui-specific domain objects
// ---------------------------------------------------------------------------

/// Broad category for typed error handling and policy decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Network,
    Authentication,
    Authorization,
    RateLimit,
    Timeout,
    InvalidInput,
    Parse,
    Tool,
    State,
    Internal,
}

/// Severity hint for UI and logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Unified envelope used when crossing subsystem boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub recoverable: bool,
    pub code: String,
    pub message: String,
}

/// User-facing coherence ladder for session health.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoherenceState {
    #[default]
    Healthy,
    GettingCrowded,
    RefreshingContext,
    VerifyingRecentWork,
    ResettingPlan,
}

// ---------------------------------------------------------------------------
// EngineEvent
// ---------------------------------------------------------------------------

/// Events emitted by the engine to update any frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    // === Streaming Events ===
    /// A new message block has started.
    MessageStarted {
        #[allow(dead_code)]
        index: usize,
    },

    /// Incremental text content delta.
    MessageDelta {
        #[allow(dead_code)]
        index: usize,
        content: String,
    },

    /// Message block completed.
    MessageComplete {
        #[allow(dead_code)]
        index: usize,
    },

    /// Thinking block started.
    ThinkingStarted {
        #[allow(dead_code)]
        index: usize,
    },

    /// Incremental thinking content delta.
    ThinkingDelta {
        #[allow(dead_code)]
        index: usize,
        content: String,
    },

    /// Thinking block completed.
    ThinkingComplete {
        #[allow(dead_code)]
        index: usize,
    },

    // === Tool Events ===
    /// Tool call initiated.
    ToolCallStarted {
        id: String,
        name: String,
        input: Value,
    },

    /// Tool execution progress (for long-running tools).
    #[allow(dead_code)]
    ToolCallProgress { id: String, output: String },

    /// Tool call completed.
    ToolCallComplete {
        id: String,
        name: String,
        result: Result<ToolResult, ToolErrorPayload>,
    },

    // === Turn Lifecycle ===
    /// A new turn has started (user sent a message).
    TurnStarted { turn_id: String },

    /// The turn is complete (no more tool calls).
    TurnComplete {
        /// Serialized usage statistics.
        usage: Value,
        status: TurnOutcomeStatus,
        error: Option<String>,
        /// Tool catalog sent with this turn's model request (serialized).
        tool_catalog: Option<Value>,
        /// API base URL used by this turn's client.
        base_url: Option<String>,
    },

    /// Context compaction started.
    CompactionStarted {
        id: String,
        auto: bool,
        message: String,
    },

    /// Context compaction completed.
    CompactionCompleted {
        id: String,
        auto: bool,
        message: String,
        /// Number of messages before compaction.
        #[allow(dead_code)]
        messages_before: Option<usize>,
        /// Number of messages after compaction.
        #[allow(dead_code)]
        messages_after: Option<usize>,
    },

    /// Context purge started.
    PurgeStarted {
        /// Status message for display.
        message: String,
    },

    /// Context purge completed.
    PurgeCompleted {
        /// Number of messages before purge.
        messages_before: usize,
        /// Number of messages after purge.
        messages_after: usize,
        /// How many messages were removed.
        removed_count: usize,
        /// How many replace operations were applied.
        replaced_count: usize,
        /// Summary message for display.
        message: String,
    },

    /// Context purge failed.
    PurgeFailed { message: String },

    /// Context compaction failed.
    CompactionFailed {
        id: String,
        auto: bool,
        message: String,
    },

    /// Capacity decision telemetry.
    #[allow(dead_code)]
    CapacityDecision {
        session_id: String,
        turn_id: String,
        h_hat: f64,
        c_hat: f64,
        slack: f64,
        min_slack: f64,
        violation_ratio: f64,
        p_fail: f64,
        risk_band: String,
        action: String,
        cooldown_blocked: bool,
        reason: String,
    },

    /// Capacity intervention telemetry.
    #[allow(dead_code)]
    CapacityIntervention {
        session_id: String,
        turn_id: String,
        action: String,
        before_prompt_tokens: usize,
        after_prompt_tokens: usize,
        compaction_size_reduction: usize,
        replay_outcome: Option<String>,
        replan_performed: bool,
    },

    /// Capacity memory persistence failure telemetry.
    #[allow(dead_code)]
    CapacityMemoryPersistFailed {
        session_id: String,
        turn_id: String,
        action: String,
        error: String,
    },

    /// Plain-language session coherence state.
    CoherenceState {
        state: CoherenceState,
        label: String,
        description: String,
        reason: String,
    },

    // === Sub-Agent Events ===
    /// A sub-agent has been spawned.
    AgentSpawned { id: String, prompt: String },

    /// Sub-agent progress update.
    AgentProgress { id: String, status: String },

    /// Sub-agent completed.
    AgentComplete { id: String, result: String },

    /// Sub-agent listing (serialized).
    AgentList { agents: Value },

    /// Structured sub-agent mailbox envelope. Carries the monotonic seq and
    /// the serialized mailbox message so the frontend can route each
    /// envelope to the correct in-transcript card.
    SubAgentMailbox {
        seq: u64,
        message: Value,
    },

    // === System Events ===
    /// An error occurred.
    Error {
        envelope: ErrorEnvelope,
        #[allow(dead_code)]
        recoverable: bool,
    },

    /// Status message for UI display.
    Status { message: String },

    /// Pause frontend input events (for interactive subprocesses).
    PauseEvents,

    /// Resume frontend input events after subprocess completion.
    ResumeEvents,

    /// Request user approval for a tool call.
    ApprovalRequired {
        id: String,
        tool_name: String,
        description: String,
        /// Tool parameters for approval display.
        input: Value,
        /// Exact-argument fingerprint, used to scope *denials*.
        approval_key: String,
        /// Lossy / arity-aware fingerprint, used to scope *approvals*.
        approval_grouping_key: String,
        /// The model's explanation of intent before invoking write tools.
        intent_summary: Option<String>,
    },

    /// Request user input for a tool call (serialized payload).
    UserInputRequired {
        id: String,
        request: Value,
    },

    /// Authoritative API conversation state from the engine session.
    ///
    /// The frontend receives granular display events, but those are not always
    /// a lossless representation of the API transcript. This event carries the
    /// full serialized session state for persistence.
    SessionUpdated {
        session_id: String,
        /// Serialized conversation messages.
        messages: Value,
        /// Serialized system prompt.
        system_prompt: Value,
        model: String,
        workspace: PathBuf,
    },

    /// Request user decision after sandbox denial.
    #[allow(dead_code)]
    ElevationRequired {
        tool_id: String,
        tool_name: String,
        command: Option<String>,
        denial_reason: String,
        blocked_network: bool,
        blocked_write: bool,
    },

    // === Prefix-Cache Stability Events ===
    /// The prefix (system prompt + tool specs) changed between turns,
    /// which invalidates DeepSeek's KV prefix cache.
    PrefixCacheChange {
        /// Human-readable description of what changed.
        description: String,
        /// Whether the system prompt component changed.
        system_prompt_changed: bool,
        /// Whether the tool set component changed.
        tools_changed: bool,
        /// Overall prefix stability percentage (100 = fully stable).
        stability_pct: u32,
        /// True when the prefix actually changed (cache invalidated).
        changed: bool,
        /// Current pinned prefix combined hash (SHA-256, 64 hex chars).
        pinned_combined_hash: String,
    },

    // === Notification Events ===
    /// A notification that the frontend should surface in a platform-
    /// appropriate way (taskbar indicator, title animation, etc.).
    Notification {
        kind: NotificationKind,
        message: String,
    },
}

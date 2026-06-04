//! UI-agnostic shell state management.
//!
//! Provides an Elm-style reducer (`ShellState` + `reduce`) that any frontend
//! (TUI, Web, GUI, Android) can use to manage application state without
//! depending on any specific UI framework.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use codewhale_engine::{AppMode, ApprovalMode, EngineEvent, EngineOp};

/// Summary of a conversation message for state tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    pub role: String,
    pub content_preview: String,
    pub turn_id: Option<String>,
}

/// Summary of an active tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTool {
    pub id: String,
    pub name: String,
}

/// Pending approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub tool_name: String,
    pub description: String,
}

/// Pending user input request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUserInput {
    pub id: String,
    pub prompt: String,
}

/// Summary of a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentSummary {
    pub id: String,
    pub status: String,
    pub prompt_preview: String,
}

/// The complete UI-agnostic shell state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellState {
    // Session
    pub session_id: String,
    pub messages: Vec<MessageSummary>,
    pub model: String,
    pub mode: AppMode,
    pub approval_mode: ApprovalMode,
    pub workspace: PathBuf,

    // Streaming
    pub is_streaming: bool,
    pub streaming_content: String,
    pub thinking_content: Option<String>,

    // Tools
    pub active_tools: Vec<ActiveTool>,
    pub pending_approvals: Vec<PendingApproval>,
    pub pending_user_inputs: Vec<PendingUserInput>,

    // Sub-agents
    pub sub_agents: Vec<SubAgentSummary>,

    // Status
    pub is_paused: bool,
    pub status_line: String,
    pub last_error: Option<String>,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            messages: Vec::new(),
            model: String::new(),
            mode: AppMode::Agent,
            approval_mode: ApprovalMode::default(),
            workspace: PathBuf::new(),
            is_streaming: false,
            streaming_content: String::new(),
            thinking_content: None,
            active_tools: Vec::new(),
            pending_approvals: Vec::new(),
            pending_user_inputs: Vec::new(),
            sub_agents: Vec::new(),
            is_paused: false,
            status_line: "ready".to_string(),
            last_error: None,
        }
    }
}

/// Side effects emitted by the reducer.
#[derive(Debug, Clone)]
pub enum ShellEffect {
    /// Send an operation to the engine.
    SendOp(EngineOp),
    /// Submit an approval decision.
    SubmitApproval { id: String, approved: bool },
    /// Submit user input.
    SubmitUserInput { id: String, response: String },
    /// Persist a checkpoint.
    PersistCheckpoint,
    /// Request UI redraw.
    RequestRedraw,
    /// Display a notification.
    Notify { kind: String, message: String },
}

/// User actions that any frontend can map to.
#[derive(Debug, Clone)]
pub enum UserAction {
    SubmitPrompt { content: String },
    Approve { id: String },
    Deny { id: String },
    Cancel,
    ChangeMode { mode: AppMode },
    SetModel { model: String },
    ScrollUp,
    ScrollDown,
    ToggleThinking,
    CompactContext,
    PurgeContext,
}

/// Elm-style reducer: pure function, no side effects.
/// Returns a list of effects that the frontend should execute.
pub fn reduce(state: &mut ShellState, event: EngineEvent) -> Vec<ShellEffect> {
    match event {
        EngineEvent::MessageDelta { content, .. } => {
            state.is_streaming = true;
            state.streaming_content.push_str(&content);
            state.status_line = "streaming".to_string();
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::MessageComplete { .. } => {
            state.is_streaming = false;
            state.streaming_content.clear();
            state.status_line = "ready".to_string();
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::ThinkingDelta { content, .. } => {
            state.thinking_content = Some(
                state.thinking_content.take().unwrap_or_default() + &content
            );
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::ThinkingComplete { .. } => {
            state.thinking_content = None;
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::ToolCallStarted { id, name, .. } => {
            state.active_tools.push(ActiveTool { id, name });
            state.status_line = "tool running".to_string();
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::ToolCallComplete { id, .. } => {
            state.active_tools.retain(|t| t.id != id);
            if state.active_tools.is_empty() {
                state.status_line = "ready".to_string();
            }
            vec![ShellEffect::RequestRedraw, ShellEffect::PersistCheckpoint]
        }
        EngineEvent::TurnStarted { turn_id } => {
            state.status_line = format!("turn {turn_id} started");
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::TurnComplete { .. } => {
            state.is_streaming = false;
            state.streaming_content.clear();
            state.status_line = "turn complete".to_string();
            vec![ShellEffect::RequestRedraw, ShellEffect::PersistCheckpoint]
        }
        EngineEvent::ApprovalRequired { id, tool_name, description, .. } => {
            state.pending_approvals.push(PendingApproval {
                id,
                tool_name,
                description,
            });
            state.status_line = "approval required".to_string();
            vec![ShellEffect::RequestRedraw, ShellEffect::Notify {
                kind: "approval".to_string(),
                message: "Tool approval required".to_string(),
            }]
        }
        EngineEvent::UserInputRequired { id, request } => {
            let prompt = request.as_object()
                .and_then(|o| o.get("prompt"))
                .and_then(|v| v.as_str())
                .unwrap_or("Input required")
                .to_string();
            state.pending_user_inputs.push(PendingUserInput { id, prompt });
            state.status_line = "user input required".to_string();
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::AgentSpawned { id, prompt } => {
            state.sub_agents.push(SubAgentSummary {
                id,
                status: "running".to_string(),
                prompt_preview: prompt.chars().take(80).collect(),
            });
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::AgentComplete { id, .. } => {
            if let Some(agent) = state.sub_agents.iter_mut().find(|a| a.id == id) {
                agent.status = "completed".to_string();
            }
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::Error { envelope, .. } => {
            state.last_error = Some(envelope.message.clone());
            state.status_line = format!("error: {}", envelope.message);
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::Status { message } => {
            state.status_line = message;
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::Notification { kind, message } => {
            vec![ShellEffect::Notify {
                kind: format!("{kind:?}").to_lowercase(),
                message,
            }]
        }
        EngineEvent::SessionUpdated { session_id, .. } => {
            state.session_id = session_id;
            vec![ShellEffect::PersistCheckpoint]
        }
        EngineEvent::PauseEvents => {
            state.is_paused = true;
            vec![ShellEffect::RequestRedraw]
        }
        EngineEvent::ResumeEvents => {
            state.is_paused = false;
            vec![ShellEffect::RequestRedraw]
        }
        // Default: just request redraw for unhandled events
        _ => vec![ShellEffect::RequestRedraw],
    }
}

/// Map user actions to shell effects.
pub fn handle_user_action(state: &ShellState, action: UserAction) -> Vec<ShellEffect> {
    match action {
        UserAction::SubmitPrompt { content } => vec![ShellEffect::SendOp(EngineOp::SendMessage {
            content,
            mode: state.mode,
            model: state.model.clone(),
            goal_objective: None,
            reasoning_effort: None,
            reasoning_effort_auto: false,
            auto_model: false,
            allow_shell: true,
            trust_mode: false,
            auto_approve: false,
            approval_mode: state.approval_mode,
            translation_enabled: false,
            show_thinking: false,
            allowed_tools: None,
            hook_executor_name: None,
        })],
        UserAction::Approve { id } => vec![ShellEffect::SubmitApproval { id, approved: true }],
        UserAction::Deny { id } => vec![ShellEffect::SubmitApproval { id, approved: false }],
        UserAction::Cancel => vec![ShellEffect::SendOp(EngineOp::CancelRequest)],
        UserAction::ChangeMode { mode } => vec![ShellEffect::SendOp(EngineOp::ChangeMode { mode })],
        UserAction::SetModel { model } => vec![ShellEffect::SendOp(EngineOp::SetModel {
            model,
            mode: state.mode,
        })],
        UserAction::ScrollUp | UserAction::ScrollDown | UserAction::ToggleThinking => {
            vec![ShellEffect::RequestRedraw]
        }
        UserAction::CompactContext => vec![ShellEffect::SendOp(EngineOp::CompactContext)],
        UserAction::PurgeContext => vec![ShellEffect::SendOp(EngineOp::PurgeContext)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_agent_mode() {
        let state = ShellState::default();
        assert_eq!(state.mode, AppMode::Agent);
        assert_eq!(state.approval_mode, ApprovalMode::Suggest);
        assert!(!state.is_streaming);
        assert!(state.active_tools.is_empty());
        assert!(state.pending_approvals.is_empty());
    }

    #[test]
    fn message_delta_updates_streaming() {
        let mut state = ShellState::default();
        let effects = reduce(&mut state, EngineEvent::MessageDelta {
            index: 0,
            content: "hello".to_string(),
        });
        assert!(state.is_streaming);
        assert_eq!(state.streaming_content, "hello");
        assert!(effects.iter().any(|e| matches!(e, ShellEffect::RequestRedraw)));
    }

    #[test]
    fn approval_required_adds_pending() {
        let mut state = ShellState::default();
        reduce(&mut state, EngineEvent::ApprovalRequired {
            id: "1".to_string(),
            tool_name: "exec_shell".to_string(),
            description: "Run command".to_string(),
            input: serde_json::Value::Null,
            approval_key: String::new(),
            approval_grouping_key: String::new(),
            intent_summary: None,
        });
        assert_eq!(state.pending_approvals.len(), 1);
    }

    #[test]
    fn submit_prompt_creates_send_op() {
        let state = ShellState::default();
        let effects = handle_user_action(&state, UserAction::SubmitPrompt {
            content: "hello".to_string(),
        });
        assert!(matches!(effects[0], ShellEffect::SendOp(EngineOp::SendMessage { .. })));
    }
}

use codewhale_shell::{ShellState, ShellEffect, UserAction, reduce, handle_user_action};
use codewhale_engine::{AppMode, ApprovalMode, EngineEvent};

#[test]
fn reducer_produces_stable_snapshot_for_core_workflow() {
    let mut state = ShellState::default();

    // Simulate a streaming message
    reduce(&mut state, EngineEvent::MessageDelta {
        index: 0,
        content: "hello".to_string(),
    });
    assert!(state.is_streaming);
    assert_eq!(state.streaming_content, "hello");

    // Simulate a tool call start
    reduce(&mut state, EngineEvent::ToolCallStarted {
        id: "t1".to_string(),
        name: "web.search".to_string(),
        input: serde_json::Value::Null,
    });
    assert_eq!(state.active_tools.len(), 1);
    assert_eq!(state.active_tools[0].id, "t1");

    // Simulate approval request
    reduce(&mut state, EngineEvent::ApprovalRequired {
        id: "a1".to_string(),
        tool_name: "exec_shell".to_string(),
        description: "Run command".to_string(),
        input: serde_json::Value::Null,
        approval_key: String::new(),
        approval_grouping_key: String::new(),
        intent_summary: None,
    });
    assert_eq!(state.pending_approvals.len(), 1);

    // Simulate message complete
    reduce(&mut state, EngineEvent::MessageComplete { index: 0 });
    assert!(!state.is_streaming);
    assert!(state.streaming_content.is_empty());

    // Simulate pause/resume
    reduce(&mut state, EngineEvent::PauseEvents);
    assert!(state.is_paused);
    reduce(&mut state, EngineEvent::ResumeEvents);
    assert!(!state.is_paused);

    // Verify final state
    assert_eq!(state.mode, AppMode::Agent);
    assert_eq!(state.approval_mode, ApprovalMode::Suggest);
}

#[test]
fn user_action_submit_prompt_produces_send_op() {
    let state = ShellState::default();
    let effects = handle_user_action(&state, UserAction::SubmitPrompt {
        content: "test prompt".to_string(),
    });
    assert!(matches!(effects[0], ShellEffect::SendOp(_)));
}

//! Adapter that bridges the synchronous `Runtime` to the async `Engine` trait.
//!
//! `RuntimeEngine` wraps a `Runtime` inside `Arc<Mutex<Runtime>>` and spawns
//! a background tokio task that receives `EngineOp`s, converts them to
//! `ThreadRequest`s, calls `Runtime::handle_thread`, and forwards the
//! resulting `EventFrame`s as `EngineEvent`s.

use std::sync::Arc;

use anyhow::{Result, bail};
use async_trait::async_trait;
use codewhale_engine::{
    ApprovalDecision, CancelReason, Engine, EngineEvent, EngineOp, ToolErrorPayload,
    TurnOutcomeStatus, UserInputDecision,
};
use codewhale_engine::events::{ErrorCategory, ErrorEnvelope, ErrorSeverity};
use codewhale_protocol::{EventFrame, ResponseChannel, ThreadRequest};
use codewhale_tools::ToolResult;
use serde_json::Value;
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::Runtime;

// ---------------------------------------------------------------------------
// EventFrame → EngineEvent conversion (free function to avoid orphan rules)
// ---------------------------------------------------------------------------

fn event_frame_to_engine_event(frame: EventFrame) -> EngineEvent {
    match frame {
        EventFrame::ResponseStart { .. } => EngineEvent::MessageStarted { index: 0 },
        EventFrame::ResponseDelta { delta, channel, .. } => match channel {
            ResponseChannel::Reasoning => EngineEvent::ThinkingDelta { index: 0, content: delta },
            _ => EngineEvent::MessageDelta { index: 0, content: delta },
        },
        EventFrame::ResponseEnd { .. } => EngineEvent::MessageComplete { index: 0 },
        EventFrame::ToolCallStart { tool_name, arguments, .. } => {
            EngineEvent::ToolCallStarted {
                id: String::new(),
                name: tool_name,
                input: arguments,
            }
        }
        EventFrame::ToolCallResult { tool_name, output, .. } => {
            EngineEvent::ToolCallComplete {
                id: String::new(),
                name: tool_name,
                result: Ok(ToolResult::success(output.to_string())),
            }
        }
        EventFrame::TurnStarted { turn_id } => EngineEvent::TurnStarted { turn_id },
        EventFrame::TurnComplete { .. } => EngineEvent::TurnComplete {
            usage: Value::Null,
            status: TurnOutcomeStatus::Completed,
            error: None,
            tool_catalog: None,
            base_url: None,
        },
        EventFrame::TurnAborted { reason, .. } => EngineEvent::TurnComplete {
            usage: Value::Null,
            status: TurnOutcomeStatus::Interrupted,
            error: Some(reason),
            tool_catalog: None,
            base_url: None,
        },
        EventFrame::Error { message, .. } => EngineEvent::Error {
            envelope: ErrorEnvelope {
                category: ErrorCategory::Internal,
                severity: ErrorSeverity::Error,
                recoverable: true,
                code: String::new(),
                message,
            },
            recoverable: true,
        },
        EventFrame::ExecApprovalRequest { request } => EngineEvent::ApprovalRequired {
            id: request.approval_id,
            tool_name: String::new(),
            description: request.reason,
            input: Value::Null,
            approval_key: String::new(),
            approval_grouping_key: String::new(),
            intent_summary: None,
        },
        EventFrame::ApplyPatchApprovalRequest { request } => EngineEvent::ApprovalRequired {
            id: request.approval_id,
            tool_name: "patch".to_string(),
            description: request.reason,
            input: Value::Null,
            approval_key: String::new(),
            approval_grouping_key: String::new(),
            intent_summary: None,
        },
        EventFrame::McpStartupUpdate { update } => EngineEvent::Status {
            message: format!("MCP {}: {:?}", update.server_name, update.status),
        },
        EventFrame::McpStartupComplete { summary } => EngineEvent::Status {
            message: format!(
                "MCP startup: {} ready, {} failed",
                summary.ready.len(),
                summary.failed.len()
            ),
        },
        EventFrame::McpToolCallBegin { server_name, tool_name } => {
            EngineEvent::ToolCallStarted {
                id: format!("mcp-{}-{}", server_name, tool_name),
                name: format!("mcp.{}.{}", server_name, tool_name),
                input: Value::Null,
            }
        }
        EventFrame::McpToolCallEnd { server_name, tool_name, ok } => {
            EngineEvent::ToolCallComplete {
                id: format!("mcp-{}-{}", server_name, tool_name),
                name: format!("mcp.{}.{}", server_name, tool_name),
                result: if ok {
                    Ok(ToolResult::success("ok"))
                } else {
                    Err(ToolErrorPayload::ExecutionFailed {
                        message: "MCP tool call failed".into(),
                    })
                },
            }
        }
        EventFrame::ElicitationRequest { request_id, prompt, .. } => {
            EngineEvent::UserInputRequired {
                id: request_id,
                request: Value::String(prompt),
            }
        }
        EventFrame::ExecCommandBegin { command, .. } => EngineEvent::ToolCallProgress {
            id: String::new(),
            output: format!("$ {}", command),
        },
        EventFrame::ExecCommandOutputDelta { delta, .. } => EngineEvent::ToolCallProgress {
            id: String::new(),
            output: delta,
        },
        EventFrame::ExecCommandEnd { exit_code, .. } => EngineEvent::Status {
            message: format!("exit: {}", exit_code),
        },
        EventFrame::PatchApplyBegin { path } => EngineEvent::ToolCallProgress {
            id: String::new(),
            output: format!("patching: {}", path),
        },
        EventFrame::PatchApplyEnd { path, ok } => EngineEvent::Status {
            message: format!("patch {}: {}", path, if ok { "ok" } else { "failed" }),
        },
    }
}

// ---------------------------------------------------------------------------
// EngineOp → ThreadRequest conversion
// ---------------------------------------------------------------------------

fn engine_op_to_thread_request(op: &EngineOp) -> Result<ThreadRequest> {
    match op {
        EngineOp::SendMessage { content, .. } => Ok(ThreadRequest::Message {
            thread_id: String::new(),
            input: content.clone(),
        }),
        EngineOp::RunShellCommand { command, .. } => Ok(ThreadRequest::Message {
            thread_id: String::new(),
            input: command.clone(),
        }),
        _ => bail!("EngineOp {:?} not yet supported via Runtime", op),
    }
}

// ---------------------------------------------------------------------------
// RuntimeEngine
// ---------------------------------------------------------------------------

/// Adapter that bridges a synchronous [`Runtime`] to the async [`Engine`] trait.
///
/// Internally holds the `Runtime` behind an `Arc<tokio::sync::Mutex>` and
/// spawns a background tokio task that:
/// 1. Receives `EngineOp`s from the `tx_op` channel.
/// 2. Converts them to `ThreadRequest`s.
/// 3. Calls `Runtime::handle_thread`.
/// 4. Forwards resulting `EventFrame`s as `EngineEvent`s.
pub struct RuntimeEngine {
    tx_op: mpsc::Sender<EngineOp>,
    rx_event: Arc<RwLock<mpsc::Receiver<EngineEvent>>>,
    cancel_token: Arc<CancellationToken>,
    cancel_reason: Arc<Mutex<Option<CancelReason>>>,
    tx_approval: mpsc::Sender<ApprovalDecision>,
    tx_user_input: mpsc::Sender<UserInputDecision>,
}

impl RuntimeEngine {
    /// Create a new `RuntimeEngine` wrapping the given [`Runtime`].
    ///
    /// Spawns a background task that processes incoming operations.
    pub fn new(runtime: Runtime) -> Self {
        let (tx_op, mut rx_op) = mpsc::channel::<EngineOp>(64);
        let (tx_event, rx_event) = mpsc::channel::<EngineEvent>(256);
        let (tx_approval, _rx_approval) = mpsc::channel::<ApprovalDecision>(16);
        let (tx_user_input, _rx_user_input) = mpsc::channel::<UserInputDecision>(16);

        let cancel_token = Arc::new(CancellationToken::new());
        let cancel_reason: Arc<Mutex<Option<CancelReason>>> =
            Arc::new(Mutex::new(None));

        let runtime = Arc::new(Mutex::new(runtime));
        let cancel_token_bg = cancel_token.clone();
        let cancel_reason_bg = cancel_reason.clone();

        // Spawn the background processing task.
        tokio::spawn(async move {
            loop {
                let op = tokio::select! {
                    op = rx_op.recv() => match op {
                        Some(op) => op,
                        None => break,
                    },
                    _ = cancel_token_bg.cancelled() => {
                        let _ = tx_event.send(EngineEvent::Status {
                            message: "cancelled".to_string(),
                        }).await;
                        continue;
                    }
                };

                match &op {
                    EngineOp::Shutdown => break,
                    EngineOp::CancelRequest => {
                        cancel_token_bg.cancel();
                        *cancel_reason_bg.lock().await = Some(CancelReason::User);
                        let _ = tx_event.send(EngineEvent::Status {
                            message: "cancelled".to_string(),
                        }).await;
                        continue;
                    }
                    EngineOp::ApproveToolCall { id } => {
                        let _ = tx_event.send(EngineEvent::Status {
                            message: format!("approval approved: {}", id),
                        }).await;
                        continue;
                    }
                    EngineOp::DenyToolCall { id } => {
                        let _ = tx_event.send(EngineEvent::Status {
                            message: format!("approval denied: {}", id),
                        }).await;
                        continue;
                    }
                    _ => {}
                }

                // Convert EngineOp → ThreadRequest.
                let thread_req = match engine_op_to_thread_request(&op) {
                    Ok(req) => req,
                    Err(e) => {
                        let _ = tx_event.send(EngineEvent::Error {
                            envelope: ErrorEnvelope {
                                category: ErrorCategory::Internal,
                                severity: ErrorSeverity::Warning,
                                recoverable: true,
                                code: "unsupported_op".to_string(),
                                message: e.to_string(),
                            },
                            recoverable: true,
                        }).await;
                        continue;
                    }
                };

                // Call Runtime::handle_thread under the tokio mutex lock.
                // We hold the lock across the await so the &mut borrow is valid.
                let mut guard = runtime.lock().await;
                let result = guard.handle_thread(thread_req).await;
                // Drop the lock explicitly before sending events.
                drop(guard);

                match result {
                    Ok(thread_response) => {
                        for frame in thread_response.events {
                            let event = event_frame_to_engine_event(frame);
                            let _ = tx_event.send(event).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx_event.send(EngineEvent::Error {
                            envelope: ErrorEnvelope {
                                category: ErrorCategory::Internal,
                                severity: ErrorSeverity::Error,
                                recoverable: true,
                                code: "runtime_error".to_string(),
                                message: e.to_string(),
                            },
                            recoverable: true,
                        }).await;
                    }
                }
            }
        });

        Self {
            tx_op,
            rx_event: Arc::new(RwLock::new(rx_event)),
            cancel_token,
            cancel_reason,
            tx_approval,
            tx_user_input,
        }
    }
}

#[async_trait]
impl Engine for RuntimeEngine {
    async fn send_op(&self, op: EngineOp) -> Result<()> {
        self.tx_op.send(op).await?;
        Ok(())
    }

    async fn recv_event(&self) -> Option<EngineEvent> {
        let mut rx = self.rx_event.write().await;
        rx.try_recv().ok()
    }

    async fn submit_approval(&self, decision: ApprovalDecision) -> Result<()> {
        self.tx_approval.send(decision).await?;
        Ok(())
    }

    async fn submit_user_input(&self, response: UserInputDecision) -> Result<()> {
        self.tx_user_input.send(response).await?;
        Ok(())
    }

    async fn cancel(&self, reason: Option<String>) -> Result<()> {
        let cancel_reason = match reason.as_deref() {
            Some("external") => CancelReason::External,
            Some("preempted") => CancelReason::Preempted,
            Some("internal") => CancelReason::Internal,
            _ => CancelReason::User,
        };
        *self.cancel_reason.lock().await = Some(cancel_reason);
        self.cancel_token.cancel();
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.tx_op.send(EngineOp::Shutdown).await?;
        Ok(())
    }
}

//! Channel-based [`EngineHandle`] that implements the [`Engine`] trait.
//!
//! `EngineHandle` is the concrete, UI-agnostic handle that all frontends
//! use to communicate with the background engine task. It wraps mpsc
//! channels and a shared cancellation token.

use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::engine_api::{
    ApprovalDecision, CancelReason, Engine, UserInputDecision,
};
use crate::events::EngineEvent;
use crate::ops::EngineOp;

/// Handle to communicate with the engine.
///
/// Clonable — all clones share the same channel endpoints and cancellation
/// token. The `Engine` trait methods are the primary API; the public
/// channel fields exist for backwards-compatible TUI code that constructs
/// the handle from raw channels.
#[derive(Clone)]
pub struct EngineHandle {
    /// Send operations to the engine.
    pub tx_op: mpsc::Sender<EngineOp>,
    /// Receive events from the engine.
    pub rx_event: Arc<RwLock<mpsc::Receiver<EngineEvent>>>,
    /// Shared pointer to the cancellation token for the current request.
    cancel_token: Arc<StdMutex<CancellationToken>>,
    /// Latched reason for the most recent cancellation. Read by the
    /// approval / user-input handlers to enrich their error strings.
    /// Cleared by the engine when a fresh turn starts.
    cancel_reason: Arc<StdMutex<Option<CancelReason>>>,
    /// Send approval decisions to the engine.
    tx_approval: mpsc::Sender<ApprovalDecision>,
    /// Send user input responses to the engine.
    tx_user_input: mpsc::Sender<UserInputDecision>,
    /// Send steer input for an in-flight turn.
    tx_steer: mpsc::Sender<String>,
}

impl EngineHandle {
    /// Create a new `EngineHandle` from channel endpoints.
    ///
    /// This is the primary constructor used by the engine when it creates
    /// its own channels during initialization.
    pub fn new(
        tx_op: mpsc::Sender<EngineOp>,
        rx_event: mpsc::Receiver<EngineEvent>,
        cancel_token: Arc<StdMutex<CancellationToken>>,
        cancel_reason: Arc<StdMutex<Option<CancelReason>>>,
        tx_approval: mpsc::Sender<ApprovalDecision>,
        tx_user_input: mpsc::Sender<UserInputDecision>,
        tx_steer: mpsc::Sender<String>,
    ) -> Self {
        Self {
            tx_op,
            rx_event: Arc::new(RwLock::new(rx_event)),
            cancel_token,
            cancel_reason,
            tx_approval,
            tx_user_input,
            tx_steer,
        }
    }

    /// Cancel the current request (user-initiated path — keeps the
    /// public `cancel()` signature stable). Equivalent to
    /// `cancel_with_reason(CancelReason::User)`.
    pub fn cancel(&self) {
        self.cancel_with_reason(CancelReason::User);
    }

    /// Cancel the current request and latch the reason so downstream
    /// "request cancelled" error messages can name a cause.
    pub fn cancel_with_reason(&self, reason: CancelReason) {
        match self.cancel_reason.lock() {
            Ok(mut slot) => *slot = Some(reason),
            Err(poisoned) => *poisoned.into_inner() = Some(reason),
        }
        match self.cancel_token.lock() {
            Ok(token) => token.cancel(),
            Err(poisoned) => poisoned.into_inner().cancel(),
        }
    }

    /// Check if a request is currently cancelled.
    #[must_use]
    #[allow(dead_code)]
    pub fn is_cancelled(&self) -> bool {
        match self.cancel_token.lock() {
            Ok(token) => token.is_cancelled(),
            Err(poisoned) => poisoned.into_inner().is_cancelled(),
        }
    }

    /// Approve a pending tool call.
    pub async fn approve_tool_call(&self, id: impl Into<String>) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::Approved { id: id.into() })
            .await?;
        Ok(())
    }

    /// Deny a pending tool call.
    pub async fn deny_tool_call(&self, id: impl Into<String>) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::Denied { id: id.into() })
            .await?;
        Ok(())
    }

    /// Submit a response for request_user_input.
    pub async fn submit_user_input_response(
        &self,
        _id: impl Into<String>,
        response: UserInputDecision,
    ) -> Result<()> {
        self.tx_user_input.send(response).await?;
        Ok(())
    }

    /// Cancel a request_user_input prompt.
    pub async fn cancel_user_input(&self, id: impl Into<String>) -> Result<()> {
        self.tx_user_input
            .send(UserInputDecision::Cancelled { id: id.into() })
            .await?;
        Ok(())
    }

    /// Steer an in-flight turn with additional user input.
    pub async fn steer(&self, content: impl Into<String>) -> Result<()> {
        self.tx_steer.send(content.into()).await?;
        Ok(())
    }

    /// Access the shared cancel reason (for TUI compatibility).
    pub fn cancel_reason(&self) -> &Arc<StdMutex<Option<CancelReason>>> {
        &self.cancel_reason
    }

    /// Access the shared cancel token (for TUI compatibility).
    pub fn cancel_token(&self) -> &Arc<StdMutex<CancellationToken>> {
        &self.cancel_token
    }

    /// Access the approval sender (for TUI compatibility).
    pub fn tx_approval(&self) -> &mpsc::Sender<ApprovalDecision> {
        &self.tx_approval
    }

    /// Access the user-input sender (for TUI compatibility).
    pub fn tx_user_input(&self) -> &mpsc::Sender<UserInputDecision> {
        &self.tx_user_input
    }

    /// Access the steer sender (for TUI compatibility).
    pub fn tx_steer(&self) -> &mpsc::Sender<String> {
        &self.tx_steer
    }
}

#[async_trait]
impl Engine for EngineHandle {
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
        self.cancel_with_reason(cancel_reason);
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.tx_op.send(EngineOp::Shutdown).await?;
        Ok(())
    }
}

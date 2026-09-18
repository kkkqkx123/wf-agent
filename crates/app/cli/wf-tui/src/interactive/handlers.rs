//! Domain-side handler adapters bridging runtime callbacks into the session
//! event channel.
//!
//! These implement the approval and user-interaction traits the domain layer
//! invokes off-thread; their only job is to post an [`InteractiveEvent`] to the
//! running session and (for approvals) await the oneshot reply.

use tokio::sync::{mpsc, oneshot};

use serde_json::Value;

use wf_api::entity::user_interaction::{AgentUserInteractionEventRecord, UserInteractionHandler};
use wf_api::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};

use crate::interactive::InteractiveEvent;

/// Domain-side approval handler: post the request to the session channel and
/// await the oneshot reply.
pub struct TuiApprovalHandler {
    tx: mpsc::UnboundedSender<InteractiveEvent>,
}

impl TuiApprovalHandler {
    pub fn new(tx: mpsc::UnboundedSender<InteractiveEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for TuiApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(InteractiveEvent::ApprovalRequested {
                request: request.clone(),
                reply: reply_tx,
            })
            .is_err()
        {
            return ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "TUI session closed before the approval was answered",
            );
        }
        match tokio::time::timeout(crate::approval_overlay::APPROVAL_TIMEOUT, reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "approval reply channel closed",
            ),
            Err(_) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "approval timed out waiting for the user",
            ),
        }
    }
}

/// Domain-side interaction handler: forward follow-up questions to the
/// session channel. Tool approvals go through [`TuiApprovalHandler`].
pub struct TuiInteractionHandler {
    tx: mpsc::UnboundedSender<InteractiveEvent>,
}

impl TuiInteractionHandler {
    pub fn new(tx: mpsc::UnboundedSender<InteractiveEvent>) -> Self {
        Self { tx }
    }
}

impl UserInteractionHandler for TuiInteractionHandler {
    fn on_interaction(&self, _record: &AgentUserInteractionEventRecord) {}

    fn on_tool_approval_requested(&self, _execution_id: &str, _request: &Value) {
        // Approvals flow through TuiApprovalHandler.
    }

    fn on_followup_question_requested(&self, _execution_id: &str, request: &Value) {
        let interaction_id = request
            .get("interactionId")
            .or_else(|| request.get("interaction_id"))
            .or_else(|| request.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let _ = self.tx.send(InteractiveEvent::QuestionRequested {
            interaction_id,
            request: request.clone(),
        });
    }
}

//! Host-side tool approval handler backed by the user interaction
//! machinery (`request_user_approval`).
//!
//! An `InteractionApprovalHandler` implements the shared
//! `ToolApprovalHandler` contract: every tool call that reaches it opens a
//! persisted `user_interaction` record (`tool_approval`), notifies the
//! registered `UserInteractionHandler` and waits for the response. Hosts
//! opt in by constructing one handler per execution (it captures the
//! execution id for the interaction records) and attaching it to the agent
//! coordinator (`with_approval_handler`) or the workflow executor context
//! (`with_tool_approval`).

use std::sync::Arc;

use wf_execution_shared::approval::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
use wf_types::interaction::tool_approval::ToolApprovalRequestData;

use crate::infra::context::ApiContext;
use crate::workflow::approval::request_user_approval;

/// Tool approval handler resolving decisions through the persisted user
/// interaction flow.
pub struct InteractionApprovalHandler {
    ctx: Arc<ApiContext>,
    execution_id: String,
    timeout_ms: u64,
}

impl InteractionApprovalHandler {
    pub fn new(ctx: Arc<ApiContext>, execution_id: impl Into<String>, timeout_ms: u64) -> Self {
        Self {
            ctx,
            execution_id: execution_id.into(),
            timeout_ms,
        }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for InteractionApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let request_data = ToolApprovalRequestData {
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            tool_description: None,
            parameters: request.arguments.clone(),
            risk_level: None,
            pending_queue: request.pending_queue.clone(),
            batch_id: request.batch_id.clone(),
            tool_index: request.tool_index,
            total_tools: request.total_tools,
            timeout: Some(self.timeout_ms),
            security_preset: None,
        };
        match request_user_approval(
            &self.ctx,
            &self.execution_id,
            &request_data,
            self.timeout_ms,
        )
        .await
        {
            Ok((_interaction_id, response)) => {
                let mut result = ToolApprovalResult::from(response);
                result.tool_call_id = request.tool_call_id.clone();
                result
            }
            Err(err) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                format!("approval wait failed: {err}"),
            ),
        }
    }
}

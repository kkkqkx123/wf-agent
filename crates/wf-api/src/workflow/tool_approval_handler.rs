//! Host-side tool approval handler backed by the user interaction
//! machinery (`request_user_approval`).
//!
//! An `InteractionApprovalHandler` implements the shared
//! `ToolApprovalHandler` contract for the tool calls the policy engine
//! routes to a human (its `Ask` decisions): it opens a persisted
//! `user_interaction` record (`tool_approval`), notifies the registered
//! `UserInteractionHandler` and waits for the response. There is no wait
//! bound: whether a tool asks at all is decided by policy, and an open
//! request stays open until someone answers or the execution ends.
//!
//! Hosts opt in by attaching [`host_tool_approval`] per execution: the
//! effective policy options plus this handler go to the agent coordinator
//! (`with_approval_options` / `with_approval_handler`) or the workflow
//! executor context (`with_tool_approval`). Caller-supplied handlers keep
//! precedence over the host default.

use std::sync::Arc;

use wf_execution_shared::approval::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
use wf_types::interaction::tool_approval::ToolApprovalRequestData;
use wf_types::tool::approval::ToolApprovalOptions;

use crate::infra::context::ApiContext;
use crate::workflow::approval::{request_user_approval_in, ApprovalFlow};

/// Tool approval handler resolving decisions through the persisted user
/// interaction flow.
///
/// Holds clones of the context pieces the flow touches (interaction store,
/// shared event bus, registered notifier) instead of the whole context, so
/// it can be built wherever the executing application context is at hand.
pub struct InteractionApprovalHandler {
    flow: ApprovalFlow,
    execution_id: String,
}

impl InteractionApprovalHandler {
    /// Build a handler for one execution off the application context.
    pub fn new(ctx: &ApiContext, execution_id: impl Into<String>) -> Self {
        Self {
            flow: ApprovalFlow::from_context(ctx),
            execution_id: execution_id.into(),
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
            timeout: None,
            security_preset: None,
        };
        match request_user_approval_in(&self.flow, &self.execution_id, &request_data).await {
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

/// Host-default approval wiring for one execution: the effective policy
/// options plus the interaction-backed handler.
///
/// Returns `None` — keeping the library default of auto-approved tool
/// calls — when the host config is absent or disabled. Callers attach the
/// wiring only when the invocation carries no caller-supplied handler of
/// its own.
pub struct HostToolApproval {
    pub options: ToolApprovalOptions,
    pub handler: Arc<dyn ToolApprovalHandler>,
}

pub fn host_tool_approval(ctx: &ApiContext, execution_id: &str) -> Option<HostToolApproval> {
    let config = ctx.tool_approval.as_ref()?;
    if !config.enabled {
        return None;
    }
    Some(HostToolApproval {
        options: config.resolved_options(),
        handler: Arc::new(InteractionApprovalHandler::new(ctx, execution_id)),
    })
}

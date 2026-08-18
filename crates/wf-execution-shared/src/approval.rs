//! Tool-level approval contract shared by the execution engines.
//!
//! Both the agent loop and the workflow LLM node execute tools; the
//! pre-execution side-effect guard is implemented once here so both paths
//! can intercept tool calls through the same handler. The host (wf-api /
//! wf-runtime) supplies a `ToolApprovalHandler` backed by the user
//! interaction machinery; absent a handler the engines fall back to the
//! policy engine (`ToolApprovalOptions`).

use serde_json::Value;

use wf_types::interaction::tool_approval::ToolApprovalResponseData;

/// Request handed to an external tool approval handler. The interaction id
/// links the request to the asynchronous approval response channel.
#[derive(Debug, Clone)]
pub struct ToolApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub interaction_id: String,
    pub batch_id: Option<String>,
    pub tool_index: Option<u32>,
    pub total_tools: Option<u32>,
    pub pending_queue: Option<Vec<wf_types::interaction::tool_approval::PendingToolCallInfo>>,
}

#[derive(Debug, Clone)]
pub struct ToolApprovalResult {
    pub tool_call_id: String,
    pub approved: bool,
    pub edited_parameters: Option<Value>,
    pub user_instruction: Option<String>,
    pub rejection_reason: Option<String>,
}

impl ToolApprovalResult {
    pub fn approved(tool_call_id: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            approved: true,
            edited_parameters: None,
            user_instruction: None,
            rejection_reason: None,
        }
    }

    pub fn rejected(tool_call_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            approved: false,
            edited_parameters: None,
            user_instruction: None,
            rejection_reason: Some(reason.into()),
        }
    }
}

impl From<ToolApprovalResponseData> for ToolApprovalResult {
    fn from(value: ToolApprovalResponseData) -> Self {
        Self {
            tool_call_id: String::new(),
            approved: value.approved,
            edited_parameters: value.edited_parameters,
            user_instruction: value.user_instruction,
            rejection_reason: value.rejection_reason,
        }
    }
}

/// External tool approval handler registered on an execution. When absent,
/// tools are auto-approved unless explicit `ToolApprovalOptions` route them
/// through the policy engine.
#[async_trait::async_trait]
pub trait ToolApprovalHandler: Send + Sync {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult;
}

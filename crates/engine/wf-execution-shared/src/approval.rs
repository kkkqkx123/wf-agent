//! Tool-level approval contract shared by the execution engines.
//!
//! Both the agent loop and the workflow LLM node execute tools; the
//! pre-execution side-effect guard is policy-first on both paths: the
//! policy engine (`ToolApprovalOptions`) decides per tool call, denials are
//! final and never reach the handler, and only `Ask` decisions are routed
//! through the handler. The host (wf-api / wf-runtime) supplies a
//! `ToolApprovalHandler` backed by the user interaction machinery; with no
//! handler attached an `Ask` decision fails closed instead of waiting.

use serde_json::Value;

use wf_types::interaction::tool_approval::ToolApprovalResponseData;

/// Request handed to an external tool approval handler. The interaction id
/// links the request to the asynchronous approval response channel.
/// `risk_level` / `tool_description` carry the registry metadata the policy
/// engine evaluated on, so human responders and audit records see the same
/// risk context as the policy decision.
#[derive(Debug, Clone)]
pub struct ToolApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub interaction_id: String,
    pub risk_level: Option<String>,
    pub tool_description: Option<String>,
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

/// External tool approval handler registered on an execution. Consulted
/// only for the policy engine's `Ask` decisions; policy approvals and
/// denials never reach the handler. When absent, an `Ask` decision fails
/// closed; with neither options nor handler tools are auto-approved.
#[async_trait::async_trait]
pub trait ToolApprovalHandler: Send + Sync {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult;
}

/// Substitute `{{name}}` placeholders in approval messages. Unmatched
/// placeholders are kept verbatim. Shared by approval message builders so
/// every rejection and hint text uses identical substitution semantics.
pub fn apply_approval_template_variables(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (key, value) in vars {
        out = out.replace(&format!("{{{{{}}}}}", key), value);
    }
    out
}

//! Shared tool-approval handlers for the CLI frontends.
//!
//! [`LlmApprovalHandler`] answers the engine's `Ask` decisions by asking the
//! configured LLM profile whether the pending tool call is safe. It is
//! fail-closed: any gateway error or unparseable verdict denies the call.

use std::sync::Arc;

use wf_api::infra::context::ApiContext;
use wf_api::llm::generate as llm_generate;
use wf_api::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
use wf_types::llm::LlmRequest;
use wf_types::message::Message;

const VERDICT_ALLOW: &str = "ALLOW";
const VERDICT_DENY: &str = "DENY";

const SYSTEM_PROMPT: &str = "You are a tool-call safety reviewer. Given a tool name and its \
JSON arguments, decide whether executing it is safe. Reply with exactly one word: \
ALLOW if the call is routine and non-destructive, DENY otherwise. Do not explain.";

/// Approval handler that defers `Ask` decisions to the configured LLM
/// profile. Denials are fail-closed: gateway failures and ambiguous replies
/// both reject the tool call with the reason preserved for the transcript.
pub struct LlmApprovalHandler {
    ctx: Arc<ApiContext>,
    profile_id: String,
}

impl LlmApprovalHandler {
    pub fn new(ctx: Arc<ApiContext>, profile_id: impl Into<String>) -> Self {
        Self {
            ctx,
            profile_id: profile_id.into(),
        }
    }

    async fn decide(&self, request: &ToolApprovalRequest) -> Result<bool, String> {
        let prompt = format!(
            "Tool: {}\nArguments: {}\nIs it safe to execute? Reply ALLOW or DENY.",
            request.tool_name, request.arguments
        );
        let llm_request = LlmRequest {
            profile_id: self.profile_id.clone(),
            messages: vec![
                Message::system_text(SYSTEM_PROMPT.to_string()),
                Message::user_text(prompt),
            ],
            parameters: None,
            generation: None,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        };
        let result = llm_generate(&self.ctx, &llm_request)
            .await
            .map_err(|e| e.to_string())?;
        let verdict = result
            .content
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase();
        if verdict.starts_with(VERDICT_ALLOW) {
            Ok(true)
        } else if verdict.starts_with(VERDICT_DENY) {
            Ok(false)
        } else {
            Err(format!("ambiguous verdict: {verdict}"))
        }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for LlmApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        match self.decide(request).await {
            Ok(true) => ToolApprovalResult::approved(request.tool_call_id.clone()),
            Ok(false) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "denied by LLM safety review".to_string(),
            ),
            Err(reason) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                format!("LLM approval unavailable ({reason}); failing closed"),
            ),
        }
    }
}

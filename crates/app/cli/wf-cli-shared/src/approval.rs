//! Stdin tool-approval handler for the CLI frontends.
//!
//! [`StdioApprovalHandler`] answers the engine's `Ask` decisions from the
//! terminal. Model-reviewed and policy-only approvals are host-independent
//! and live in `wf_runtime::tool_approval`, where the server reuses them
//! too; what stays here is the part that is genuinely stdio-specific.
//!
//! It is fail-closed: policy denials, timeouts, EOF and any non-`y` answer
//! all reject the tool call.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use wf_api::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
use wf_runtime::tool_approval::ApprovalPolicy;

use crate::run::DiagWriter;
use crate::stdio_prompt::{parse_approval_answer, render_approval_prompt, StdioPromptSource};

/// Approval handler answering the engine's `Ask` decisions from stdin.
///
/// Policy-allowed calls (pre-authorized prefixes, low-risk tools) approve
/// immediately without prompting; every other call renders a `? APPROVE`
/// prompt to the diagnostics channel and awaits one stdin line. Timeout,
/// EOF and any non-`y` answer deny the call, so an unattended or closed
/// stdin fails closed. With `assume_yes` every call approves without
/// touching stdin (unattended runs with no human on the line).
pub struct StdioApprovalHandler {
    policy: ApprovalPolicy,
    prompt: Option<Arc<StdioPromptSource>>,
    diag: Arc<Mutex<DiagWriter>>,
    timeout: Duration,
    json: bool,
    assume_yes: bool,
}

impl StdioApprovalHandler {
    /// Interactive handler: policy fast path plus stdin prompts.
    pub fn interactive(
        policy: ApprovalPolicy,
        prompt: Arc<StdioPromptSource>,
        diag: Arc<Mutex<DiagWriter>>,
        timeout: Duration,
        json: bool,
    ) -> Self {
        Self {
            policy,
            prompt: Some(prompt),
            diag,
            timeout,
            json,
            assume_yes: false,
        }
    }

    /// Unattended handler: approves every routed call without prompting.
    pub fn assume_yes(diag: Arc<Mutex<DiagWriter>>) -> Self {
        Self {
            policy: ApprovalPolicy::new(Vec::new()),
            prompt: None,
            diag,
            timeout: Duration::from_secs(crate::stdio_prompt::DEFAULT_APPROVAL_TIMEOUT_SECS),
            json: false,
            assume_yes: true,
        }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for StdioApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        if self.assume_yes {
            let mut diag = wf_common::lock::lock_ok(self.diag.lock());
            let _ = diag.ok(&format!(
                "▲ {} (pre-authorized by --assume-yes)",
                request.tool_name
            ));
            return ToolApprovalResult::approved(request.tool_call_id.clone());
        }
        match self.policy.decide(&request.tool_name, &request.arguments) {
            wf_runtime::tool_approval::ApprovalDecision::Allow { reason } => {
                let mut diag = wf_common::lock::lock_ok(self.diag.lock());
                let _ = diag.ok(&format!("▲ {} ({reason})", request.tool_name));
                ToolApprovalResult::approved(request.tool_call_id.clone())
            }
            wf_runtime::tool_approval::ApprovalDecision::Deny { reason } => {
                let line = render_approval_prompt(
                    &request.tool_call_id,
                    &request.tool_name,
                    &reason,
                    self.json,
                );
                {
                    let mut diag = wf_common::lock::lock_ok(self.diag.lock());
                    let _ = diag.line(&line);
                }
                let answer = match self.prompt.as_ref() {
                    Some(prompt) => prompt.next_answer(self.timeout).await,
                    None => None,
                };
                match answer {
                    Some(text) if parse_approval_answer(&text) => {
                        let mut diag = wf_common::lock::lock_ok(self.diag.lock());
                        let _ = diag.ok(&format!("▲ {} (approved by user)", request.tool_name));
                        ToolApprovalResult::approved(request.tool_call_id.clone())
                    }
                    Some(_) => {
                        let reason =
                            format!("tool '{}' denied by user (stdin answer)", request.tool_name);
                        let mut diag = wf_common::lock::lock_ok(self.diag.lock());
                        let _ = diag.err(&format!("✗ {}: {reason}", request.tool_name));
                        ToolApprovalResult::rejected(request.tool_call_id.clone(), reason)
                    }
                    None => {
                        let reason = format!(
                            "tool '{}' approval timed out after {}s; failing closed",
                            request.tool_name,
                            self.timeout.as_secs()
                        );
                        let mut diag = wf_common::lock::lock_ok(self.diag.lock());
                        let _ = diag.err(&format!("✗ {}: {reason}", request.tool_name));
                        ToolApprovalResult::rejected(request.tool_call_id.clone(), reason)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_request(tool: &str) -> ToolApprovalRequest {
        ToolApprovalRequest {
            tool_call_id: "call-1".into(),
            tool_name: tool.into(),
            arguments: json!({}),
            interaction_id: "approval-1".into(),
            risk_level: None,
            tool_description: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
            pending_queue: None,
        }
    }

    fn test_diag() -> Arc<Mutex<DiagWriter>> {
        Arc::new(Mutex::new(DiagWriter::buffer()))
    }

    #[tokio::test]
    async fn policy_allowed_tools_skip_the_prompt() {
        let prompt = Arc::new(StdioPromptSource::new_for_test(vec!["n".into()]));
        let diag = test_diag();
        let handler = StdioApprovalHandler::interactive(
            ApprovalPolicy::new(Vec::new()),
            prompt,
            diag.clone(),
            Duration::from_secs(5),
            false,
        );
        let result = handler.request_approval(&test_request("read_file")).await;
        assert!(result.approved);
        assert!(wf_common::lock::lock_ok(diag.lock())
            .snapshot()
            .contains("▲ read_file"));
    }

    #[tokio::test]
    async fn stdin_yes_approves_and_no_denies() {
        let diag = test_diag();
        let yes = StdioApprovalHandler::interactive(
            ApprovalPolicy::new(Vec::new()),
            Arc::new(StdioPromptSource::new_for_test(vec!["y".into()])),
            diag.clone(),
            Duration::from_secs(5),
            false,
        );
        assert!(
            yes.request_approval(&test_request("write_file"))
                .await
                .approved
        );
        assert!(wf_common::lock::lock_ok(diag.lock())
            .snapshot()
            .contains("? APPROVE"));

        let no = StdioApprovalHandler::interactive(
            ApprovalPolicy::new(Vec::new()),
            Arc::new(StdioPromptSource::new_for_test(vec!["n".into()])),
            test_diag(),
            Duration::from_secs(5),
            false,
        );
        let denied = no.request_approval(&test_request("write_file")).await;
        assert!(!denied.approved);
        assert!(denied.rejection_reason.is_some());
    }

    #[tokio::test]
    async fn timeout_and_eof_fail_closed() {
        let handler = StdioApprovalHandler::interactive(
            ApprovalPolicy::new(Vec::new()),
            Arc::new(StdioPromptSource::new_for_test(Vec::new())),
            test_diag(),
            Duration::from_millis(20),
            true,
        );
        let denied = handler.request_approval(&test_request("write_file")).await;
        assert!(!denied.approved);
        let reason = denied.rejection_reason.unwrap_or_default();
        assert!(reason.contains("timed out"), "{reason}");
    }

    #[tokio::test]
    async fn assume_yes_approves_without_reading_stdin() {
        let handler = StdioApprovalHandler::assume_yes(test_diag());
        assert!(
            handler
                .request_approval(&test_request("write_file"))
                .await
                .approved
        );
        assert!(
            handler
                .request_approval(&test_request("unknown_tool"))
                .await
                .approved
        );
    }
}

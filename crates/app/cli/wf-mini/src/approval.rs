//! Blocking approval for the native session: policy-allowed tools pass
//! silently, everything else prompts on stderr and reads one line.

use std::io::{self, Write};
use std::time::Duration;

use wf_api::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};

use wf_cli_shared::approval_policy::{ApprovalDecision, ApprovalPolicy};

use crate::output::diag_line;

/// Seconds before an unanswered approval prompt is denied.
const APPROVAL_TIMEOUT_SECS: u64 = 120;

/// Approval handler for the native session. stdin is free while a turn
/// streams (the main loop only pumps events), so a blocking line read here
/// cannot race the prompt reader.
pub struct NativeApprovalHandler {
    policy: ApprovalPolicy,
    auto_approve: bool,
}

impl NativeApprovalHandler {
    pub fn new(approve_prefixes: Vec<String>, auto_approve: bool) -> Self {
        Self {
            policy: ApprovalPolicy::new(approve_prefixes),
            auto_approve,
        }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for NativeApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        if self.auto_approve {
            diag_line(&format!("allowed {} (--approval auto)", request.tool_name));
            return ToolApprovalResult::approved(request.tool_call_id.clone());
        }
        match self.policy.decide(&request.tool_name, &request.arguments) {
            ApprovalDecision::Allow { reason } => {
                diag_line(&format!("allowed {} ({reason})", request.tool_name));
                ToolApprovalResult::approved(request.tool_call_id.clone())
            }
            ApprovalDecision::Deny { .. } => self.prompt_user(request).await,
        }
    }
}

impl NativeApprovalHandler {
    async fn prompt_user(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let denied =
            |reason: &str| ToolApprovalResult::rejected(request.tool_call_id.clone(), reason);
        {
            let stderr = io::stderr();
            let mut err = stderr.lock();
            let _ = writeln!(
                err,
                "Allow tool '{}'? [y/N] (default N, {APPROVAL_TIMEOUT_SECS}s timeout)",
                request.tool_name
            );
            let _ = err.flush();
        }
        let answer = tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => return denied("approval interrupted"),
            result = read_answer_line() => result,
        };
        let answer = match answer {
            Ok(Some(line)) => line,
            Ok(None) => return denied("approval input closed"),
            Err(_) => return denied("approval timed out"),
        };
        if is_affirmative(&answer) {
            diag_line(&format!("allowed {}", request.tool_name));
            ToolApprovalResult::approved(request.tool_call_id.clone())
        } else {
            diag_line(&format!("denied {}", request.tool_name));
            denied("denied by user")
        }
    }
}

async fn read_answer_line() -> Result<Option<String>, tokio::time::error::Elapsed> {
    tokio::time::timeout(Duration::from_secs(APPROVAL_TIMEOUT_SECS), async {
        let mut line = String::new();
        match tokio::io::AsyncBufReadExt::read_line(
            &mut tokio::io::BufReader::new(tokio::io::stdin()),
            &mut line,
        )
        .await
        {
            Ok(0) => None,
            Ok(_) => Some(line),
            Err(_) => None,
        }
    })
    .await
}

/// Accept common affirmative answers; everything else denies.
fn is_affirmative(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_yes_variants() {
        assert!(is_affirmative("y"));
        assert!(is_affirmative("Y"));
        assert!(is_affirmative("yes\n"));
        assert!(!is_affirmative(""));
        assert!(!is_affirmative("n"));
        assert!(!is_affirmative("yolo"));
    }

    #[test]
    fn policy_allows_low_risk_without_prompt() {
        let policy = ApprovalPolicy::new(vec![]);
        assert!(matches!(
            policy.decide("read_file", &serde_json::json!({})),
            ApprovalDecision::Allow { .. }
        ));
    }

    #[test]
    fn policy_denies_sensitive_for_prompt() {
        let policy = ApprovalPolicy::new(vec![]);
        assert!(matches!(
            policy.decide("write_file", &serde_json::json!({})),
            ApprovalDecision::Deny { .. }
        ));
    }

    fn approval_request(tool_name: &str) -> ToolApprovalRequest {
        ToolApprovalRequest {
            tool_call_id: "t1".to_string(),
            tool_name: tool_name.to_string(),
            arguments: serde_json::json!({}),
            interaction_id: "i1".to_string(),
            batch_id: None,
            tool_index: None,
            total_tools: None,
            pending_queue: None,
        }
    }

    #[tokio::test]
    async fn auto_approve_allows_sensitive_tool_without_prompt() {
        let handler = NativeApprovalHandler::new(Vec::new(), true);
        let result = handler
            .request_approval(&approval_request("write_file"))
            .await;
        assert!(result.approved);
    }

    #[tokio::test]
    async fn manual_mode_allows_low_risk_tool_without_prompt() {
        let handler = NativeApprovalHandler::new(Vec::new(), false);
        let result = handler
            .request_approval(&approval_request("read_file"))
            .await;
        assert!(result.approved);
    }
}

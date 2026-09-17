//! Blocking approval for the native session: policy-allowed tools pass
//! silently, everything else prompts on stderr and takes one line from the
//! shared stdin reader.

use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

use wf_api::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};

use wf_runtime::tool_approval::{ApprovalDecision, ApprovalPolicy};

use crate::input::LineReader;
use crate::output::diag_line;

/// Seconds before an unanswered approval prompt is denied.
const APPROVAL_TIMEOUT_SECS: u64 = 120;

/// Approval handler for the native session. Answers come from the shared
/// stdin reader (the only stdin reader in the process), so a timed-out or
/// interrupted prompt leaves its late lines in the channel where the
/// session drains them instead of misreading them as the next prompt.
pub struct NativeApprovalHandler {
    policy: ApprovalPolicy,
    auto_approve: bool,
    lines: Arc<tokio::sync::Mutex<LineReader>>,
    /// Turn-cancel flag shared with the session pump. The handler sets it
    /// when the user presses Ctrl-C during a prompt; the pump treats the
    /// flag as an interrupt so the whole turn stops instead of re-prompting
    /// for the next sensitive tool.
    cancel: tokio::sync::watch::Sender<bool>,
}

impl NativeApprovalHandler {
    pub fn new(
        approve_prefixes: Vec<String>,
        auto_approve: bool,
        lines: Arc<tokio::sync::Mutex<LineReader>>,
        cancel: tokio::sync::watch::Sender<bool>,
    ) -> Self {
        Self {
            policy: ApprovalPolicy::new(approve_prefixes),
            auto_approve,
            lines,
            cancel,
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
            let risk = request
                .risk_level
                .as_deref()
                .map(|r| format!(" [{r}]"))
                .unwrap_or_default();
            let _ = writeln!(
                err,
                "Allow tool '{}{risk}'? [y/N] (default N, {APPROVAL_TIMEOUT_SECS}s timeout)",
                request.tool_name
            );
            let _ = err.flush();
        }
        let answer = tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                // Ctrl-C during a prompt means "cancel the whole turn", not
                // just this tool: raise the shared flag so the pump loop
                // treats the turn as interrupted.
                let _ = self.cancel.send(true);
                return denied("approval interrupted");
            }
            result = self.read_answer_line() => result,
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

    /// Take the first channel line as the answer. The timeout wraps the
    /// channel wait (not a raw stdin read), so cancelling it leaves nothing
    /// behind in the terminal: any late line stays queued for the session
    /// drain.
    async fn read_answer_line(&self) -> Result<Option<String>, tokio::time::error::Elapsed> {
        tokio::time::timeout(Duration::from_secs(APPROVAL_TIMEOUT_SECS), async {
            self.lines.lock().await.next_line().await
        })
        .await
    }
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
            risk_level: None,
            tool_description: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
            pending_queue: None,
        }
    }

    #[tokio::test]
    async fn auto_approve_allows_sensitive_tool_without_prompt() {
        let handler = NativeApprovalHandler::new(Vec::new(), true, test_lines(), test_cancel());
        let result = handler
            .request_approval(&approval_request("write_file"))
            .await;
        assert!(result.approved);
    }

    #[tokio::test]
    async fn manual_mode_allows_low_risk_tool_without_prompt() {
        let handler = NativeApprovalHandler::new(Vec::new(), false, test_lines(), test_cancel());
        let result = handler
            .request_approval(&approval_request("read_file"))
            .await;
        assert!(result.approved);
    }

    #[tokio::test]
    async fn approval_consumes_one_shared_line() {
        let (lines, tx) = test_channel();
        tx.send("yes\n".to_string()).unwrap();
        let handler = NativeApprovalHandler::new(Vec::new(), false, lines, test_cancel());
        let result = handler
            .request_approval(&approval_request("write_file"))
            .await;
        assert!(result.approved);
    }

    #[tokio::test]
    async fn closed_input_denies_approval() {
        let (lines, tx) = test_channel();
        drop(tx);
        let handler = NativeApprovalHandler::new(Vec::new(), false, lines, test_cancel());
        let result = handler
            .request_approval(&approval_request("write_file"))
            .await;
        assert!(!result.approved);
    }

    #[tokio::test]
    async fn cancel_flag_channel_is_wired_into_handler() {
        // The Ctrl-C branch itself cannot fire a real SIGINT in unit tests;
        // verify the handler exposes a working cancel sender that a pump
        // receiver can observe.
        let (cancel, mut cancel_rx) = tokio::sync::watch::channel(false);
        let handler = NativeApprovalHandler::new(Vec::new(), false, test_lines(), cancel);
        handler.cancel.send(true).expect("receiver alive");
        assert!(*cancel_rx.borrow_and_update());
    }

    fn test_lines() -> Arc<tokio::sync::Mutex<LineReader>> {
        let (lines, _) = test_channel();
        lines
    }

    fn test_cancel() -> tokio::sync::watch::Sender<bool> {
        tokio::sync::watch::channel(false).0
    }

    fn test_channel() -> (
        Arc<tokio::sync::Mutex<LineReader>>,
        tokio::sync::mpsc::UnboundedSender<String>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Arc::new(tokio::sync::Mutex::new(LineReader::from_receiver(rx))),
            tx,
        )
    }
}

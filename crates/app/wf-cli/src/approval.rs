//! Tool approval for the mini session: the domain-side handler plus the
//! footer approval view.
//!
//! [`MiniApprovalHandler`] implements [`ToolApprovalHandler`] for the
//! interactive form: the request is posted to the mini event loop
//! ([`MiniSessionEvent::ApprovalRequested`]) with a oneshot reply channel
//! and the handler awaits the user's key press (bounded by
//! [`APPROVAL_TIMEOUT`]). This is the interactive counterpart of the
//! headless deny policy in `run.rs` — the two forms register their own
//! handlers and never mix; the interactive form must confirm.
//!
//! [`ApprovalView`] is the pure view/state machine: it renders the tool
//! name, an arguments preview and the key hints, and maps a keymap action
//! (y/a/d/n/c) onto a [`ToolApprovalResult`]. "Allow all" / "deny" are
//! session-scoped remembers — the event loop consults
//! [`ApprovalRemembered`] to auto-answer later requests for the same tool.

use std::time::Duration;

use tokio::sync::{mpsc::UnboundedSender, oneshot};
use wf_agent::approval::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};

use crate::keymap::KeyAction;
use crate::mini::MiniSessionEvent;

/// How long the handler waits for the user before rejecting by timeout
/// (generous: an approval view is allowed to sit while the user thinks).
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(600);

/// Session-scoped approval memory: tools the user allowed / denied "for
/// the whole session" (a / n keys). Later requests for a remembered tool
/// are answered without interrupting the user again.
#[derive(Debug, Clone, Default)]
pub struct ApprovalRemembered {
    allowed: Vec<String>,
    denied: Vec<String>,
}

impl ApprovalRemembered {
    /// Record a session decision for a tool (a tool is never both allowed
    /// and denied).
    pub fn remember(&mut self, tool_name: &str, approved: bool) {
        self.allowed.retain(|t| t != tool_name);
        self.denied.retain(|t| t != tool_name);
        if approved {
            self.allowed.push(tool_name.to_string());
        } else {
            self.denied.push(tool_name.to_string());
        }
    }

    /// The remembered decision for a tool, if any.
    pub fn decision_for(&self, tool_name: &str) -> Option<bool> {
        if self.allowed.iter().any(|t| t == tool_name) {
            Some(true)
        } else if self.denied.iter().any(|t| t == tool_name) {
            Some(false)
        } else {
            None
        }
    }

    /// Drop every remembered decision (`/new` clears the session).
    pub fn clear(&mut self) {
        self.allowed.clear();
        self.denied.clear();
    }
}

/// Domain-side approval handler: post the request to the mini event loop
/// and await the oneshot reply.
pub struct MiniApprovalHandler {
    tx: UnboundedSender<MiniSessionEvent>,
}

impl MiniApprovalHandler {
    pub fn new(tx: UnboundedSender<MiniSessionEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for MiniApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(MiniSessionEvent::ApprovalRequested {
                request: request.clone(),
                reply: reply_tx,
            })
            .is_err()
        {
            return ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "mini session closed before the approval was answered",
            );
        }
        match tokio::time::timeout(APPROVAL_TIMEOUT, reply_rx).await {
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

/// The approval decision attached to each key (y/a/d/n/c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    /// Allow this call only (y).
    Approve,
    /// Allow and remember for the session (a).
    ApproveAll,
    /// Deny this call only; later calls ask again (d).
    DenyOnce,
    /// Deny and remember for the session (n).
    Deny,
    /// Cancel / dismiss (c, Esc) — mapped to a rejection.
    Cancel,
}

impl ApprovalChoice {
    /// Map a keymap action (Approval context) onto a choice.
    pub fn from_action(action: KeyAction) -> Option<Self> {
        match action {
            KeyAction::Approve => Some(Self::Approve),
            KeyAction::ApproveAll => Some(Self::ApproveAll),
            KeyAction::DenyOnce => Some(Self::DenyOnce),
            KeyAction::Deny => Some(Self::Deny),
            KeyAction::Cancel | KeyAction::Back => Some(Self::Cancel),
            _ => None,
        }
    }

    /// Whether the choice is session-scoped (remembered).
    pub fn remembered(self) -> Option<bool> {
        match self {
            Self::ApproveAll => Some(true),
            Self::Deny => Some(false),
            _ => None,
        }
    }
}

/// Pure approval view state: the pending request and its rendering /
/// decision mapping. Owned by the footer while `FooterView::Permission`
/// is active.
#[derive(Debug, Clone)]
pub struct ApprovalView {
    request: ToolApprovalRequest,
}

impl ApprovalView {
    pub fn new(request: ToolApprovalRequest) -> Self {
        Self { request }
    }

    /// The pending request.
    pub fn request(&self) -> &ToolApprovalRequest {
        &self.request
    }

    /// Title line for the view.
    pub fn title(&self) -> String {
        format!("approve tool call: {}", self.request.tool_name)
    }

    /// Compact single-line arguments preview (pretty JSON truncated).
    pub fn arguments_preview(&self, width: usize) -> String {
        let pretty =
            serde_json::to_string(&self.request.arguments).unwrap_or_else(|_| "{}".to_string());
        truncate_graphemes(&pretty, width)
    }

    /// Key hints line.
    pub fn hints(&self) -> String {
        "y allow once · a allow session · d deny once · n deny session · c cancel".to_string()
    }

    /// Apply a choice to the pending request.
    pub fn apply(&self, choice: ApprovalChoice) -> ToolApprovalResult {
        match choice {
            ApprovalChoice::Approve | ApprovalChoice::ApproveAll => {
                ToolApprovalResult::approved(self.request.tool_call_id.clone())
            }
            ApprovalChoice::DenyOnce => ToolApprovalResult::rejected(
                self.request.tool_call_id.clone(),
                "denied by the user (this call only)",
            ),
            ApprovalChoice::Deny => ToolApprovalResult::rejected(
                self.request.tool_call_id.clone(),
                "denied by the user (session)",
            ),
            ApprovalChoice::Cancel => ToolApprovalResult::rejected(
                self.request.tool_call_id.clone(),
                "cancelled by the user",
            ),
        }
    }
}

/// Truncate to `width` columns on a grapheme boundary.
fn truncate_graphemes(text: &str, width: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let mut out = String::new();
    let mut w = 0usize;
    for g in text.graphemes(true) {
        let gw = g.width();
        if w + gw > width {
            break;
        }
        out.push_str(g);
        w += gw;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(tool: &str) -> ToolApprovalRequest {
        ToolApprovalRequest {
            tool_call_id: "call-1".to_string(),
            tool_name: tool.to_string(),
            arguments: json!({ "command": "rm -rf /tmp/x" }),
            interaction_id: "ui-1".to_string(),
            batch_id: None,
            tool_index: None,
            total_tools: None,
            pending_queue: None,
        }
    }

    #[test]
    fn choices_map_from_keymap_actions() {
        assert_eq!(
            ApprovalChoice::from_action(KeyAction::Approve),
            Some(ApprovalChoice::Approve)
        );
        assert_eq!(
            ApprovalChoice::from_action(KeyAction::ApproveAll),
            Some(ApprovalChoice::ApproveAll)
        );
        assert_eq!(
            ApprovalChoice::from_action(KeyAction::DenyOnce),
            Some(ApprovalChoice::DenyOnce)
        );
        assert_eq!(
            ApprovalChoice::from_action(KeyAction::Deny),
            Some(ApprovalChoice::Deny)
        );
        assert_eq!(
            ApprovalChoice::from_action(KeyAction::Cancel),
            Some(ApprovalChoice::Cancel)
        );
        assert_eq!(ApprovalChoice::from_action(KeyAction::Submit), None);
    }

    #[test]
    fn remembered_decisions_are_session_scoped() {
        let mut mem = ApprovalRemembered::default();
        assert_eq!(mem.decision_for("bash"), None);
        mem.remember("bash", true);
        assert_eq!(mem.decision_for("bash"), Some(true));
        // A later deny overrides the earlier allow.
        mem.remember("bash", false);
        assert_eq!(mem.decision_for("bash"), Some(false));
        mem.clear();
        assert_eq!(mem.decision_for("bash"), None);
    }

    #[test]
    fn view_applies_choices_to_results() {
        let view = ApprovalView::new(request("execute_command"));
        assert!(view.apply(ApprovalChoice::Approve).approved);
        assert!(view.apply(ApprovalChoice::ApproveAll).approved);
        let denied = view.apply(ApprovalChoice::DenyOnce);
        assert!(!denied.approved);
        assert_eq!(denied.tool_call_id, "call-1");
        assert!(denied.rejection_reason.unwrap().contains("only"));
        let cancelled = view.apply(ApprovalChoice::Cancel);
        assert!(!cancelled.approved);
        assert!(cancelled.rejection_reason.unwrap().contains("cancelled"));
    }

    #[test]
    fn arguments_preview_truncates_to_width() {
        let view = ApprovalView::new(request("execute_command"));
        let short = view.arguments_preview(8);
        assert!(short.chars().count() <= 8, "{short}");
        let full = view.arguments_preview(200);
        assert!(full.contains("rm -rf"));
    }

    #[tokio::test]
    async fn handler_replies_through_the_oneshot() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let handler = MiniApprovalHandler::new(tx);
        let req = request("write_file");

        let handle = tokio::spawn(async move { handler.request_approval(&req).await });
        let event = rx.recv().await.unwrap();
        let MiniSessionEvent::ApprovalRequested { request, reply } = event else {
            panic!("expected an approval request event");
        };
        assert_eq!(request.tool_name, "write_file");
        reply.send(ToolApprovalResult::approved("call-1")).unwrap();

        let result = handle.await.unwrap();
        assert!(result.approved);
    }

    #[tokio::test]
    async fn handler_rejects_when_the_ui_is_gone() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let handler = MiniApprovalHandler::new(tx);
        let req = request("write_file");
        let handle = tokio::spawn(async move { handler.request_approval(&req).await });
        // Drop the reply channel without answering.
        if let MiniSessionEvent::ApprovalRequested { reply, .. } = rx.recv().await.unwrap() {
            drop(reply);
        }
        let result = handle.await.unwrap();
        assert!(!result.approved);
    }
}

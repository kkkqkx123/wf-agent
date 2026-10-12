//! Tool approval view for the interactive session: the pure view/state
//! machine plus session-scoped remember state.
//!
//! [`ApprovalView`] renders the tool name, its risk level and description,
//! the batch context, an arguments preview and the key hints, and maps a
//! keymap action (y/a/d/n/c) onto an [`ApprovalAnswer`]. "Allow all" /
//! "deny" are session-scoped remembers — the session event loop consults
//! [`ApprovalRemembered`] to auto-answer later requests for the same tool.
//! The request shape is the wire `ToolApprovalRequestData` shared with the
//! server; answers post to the interaction respond endpoint.

use std::time::Duration;

use wf_types::interaction::tool_approval::ToolApprovalRequestData;

use tui_core::keymap::KeyAction;

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
    request: ToolApprovalRequestData,
}

impl ApprovalView {
    pub fn new(request: ToolApprovalRequestData) -> Self {
        Self { request }
    }

    /// The pending request.
    pub fn request(&self) -> &ToolApprovalRequestData {
        &self.request
    }

    /// Title line for the view.
    pub fn title(&self) -> String {
        format!("approve tool call: {}", self.request.tool_name)
    }

    /// Risk context lines: risk level, tool description and batch position.
    /// Empty when the request carries none (older callers, tests).
    pub fn context_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(risk) = self.request.risk_level.as_deref() {
            lines.push(format!("risk: {risk}"));
        }
        if let Some(description) = self.request.tool_description.as_deref() {
            lines.push(format!("tool: {description}"));
        }
        match (self.request.tool_index, self.request.total_tools) {
            (Some(index), Some(total)) if total > 1 => {
                lines.push(format!("batch: call {} of {total}", index + 1));
            }
            _ => {}
        }
        if let Some(queue) = self.request.pending_queue.as_ref() {
            if queue.len() > 1 {
                lines.push(format!("batch: {} calls awaiting decision", queue.len()));
            }
        }
        lines
    }

    /// Compact single-line arguments preview (pretty JSON truncated).
    pub fn arguments_preview(&self, width: usize) -> String {
        let pretty =
            serde_json::to_string(&self.request.parameters).unwrap_or_else(|_| "{}".to_string());
        truncate_graphemes(&pretty, width)
    }

    /// Key hints line.
    pub fn hints(&self) -> String {
        "y allow once · a allow session · d deny once · n deny session · c cancel".to_string()
    }

    /// Apply a choice to the pending request.
    pub fn apply(&self, choice: ApprovalChoice) -> ApprovalAnswer {
        match choice {
            ApprovalChoice::Approve | ApprovalChoice::ApproveAll => ApprovalAnswer::approved(),
            ApprovalChoice::DenyOnce => {
                ApprovalAnswer::rejected("denied by the user (this call only)")
            }
            ApprovalChoice::Deny => ApprovalAnswer::rejected("denied by the user (session)"),
            ApprovalChoice::Cancel => ApprovalAnswer::rejected("cancelled by the user"),
        }
    }
}

/// The user's answer to one approval request: posts as the interaction
/// `response_data` body.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalAnswer {
    pub approved: bool,
    pub rejection_reason: Option<String>,
}

impl ApprovalAnswer {
    pub fn approved() -> Self {
        Self {
            approved: true,
            rejection_reason: None,
        }
    }

    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            approved: false,
            rejection_reason: Some(reason.into()),
        }
    }

    /// Wire body for the interaction respond endpoint.
    pub fn into_response_value(self) -> serde_json::Value {
        match self.rejection_reason {
            Some(reason) => serde_json::json!({"approved": false, "rejection_reason": reason}),
            None => serde_json::json!({"approved": self.approved}),
        }
    }
}

impl tui_core::renderable::Renderable for ApprovalView {
    fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        use ratatui::text::{Line, Span};

        let width = usize::from(area.width.max(1));
        let mut lines: Vec<Line<'static>> = vec![
            Line::from(Span::raw(self.title())),
            Line::from(Span::raw("")),
        ];
        let context = self.context_lines();
        for line in &context {
            lines.push(Line::from(Span::raw(line.clone())));
        }
        if !context.is_empty() {
            lines.push(Line::from(Span::raw("")));
        }
        let preview_rows = area.height.saturating_sub(4) as usize;
        let preview = self.arguments_preview(width.saturating_sub(2));
        let mut remaining = preview_rows;
        for chunk in crate::footer::wrap_columns(&preview, width.saturating_sub(2)) {
            if remaining == 0 {
                break;
            }
            lines.push(Line::from(Span::raw(chunk)));
            remaining -= 1;
        }
        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(Span::raw(self.hints())));

        for (i, line) in lines.iter().enumerate() {
            if i as u16 >= area.height {
                break;
            }
            let row = ratatui::layout::Rect {
                x: area.x,
                y: area.y + i as u16,
                width: area.width,
                height: 1,
            };
            crate::footer::render_line_into(row, buf, line);
        }
    }

    fn desired_height(&self, _width: u16) -> u16 {
        12 + self.context_lines().len() as u16
    }
}
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

    fn request(tool: &str) -> ToolApprovalRequestData {
        ToolApprovalRequestData {
            tool_call_id: "call-1".to_string(),
            tool_name: tool.to_string(),
            tool_description: None,
            parameters: json!({ "command": "rm -rf /tmp/x" }),
            risk_level: None,
            pending_queue: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
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
}

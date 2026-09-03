//! Follow-up question view for the mini session: the domain-side
//! interaction handler plus the footer question view.
//!
//! [`MiniInteractionHandler`] implements the wf-api
//! [`UserInteractionHandler`] for the interactive form: a follow-up question
//! request is posted to the mini event loop
//! ([`MiniSessionEvent::QuestionRequested`]) and rendered as
//! [`FooterView::Question`]; the answer travels back through
//! `agent_user_interaction::respond_interaction` (the domain's own reply
//! channel), never through a private one. Tool approvals are a no-op here —
//! they ride the [`crate::approval::MiniApprovalHandler`] channel instead.
//!
//! [`QuestionView`] is the pure view/state machine: it parses the request
//! payload (prompt / options / multi-select flag), tracks the toggled picks,
//! maps keymap actions (`Pick(1..=9)` / `Select` / `Cancel`) onto a
//! [`QuestionOutcome`] and renders the option list with selection markers.

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use wf_api::entity::user_interaction::{AgentUserInteractionEventRecord, UserInteractionHandler};

use crate::keymap::KeyAction;
use crate::mini::MiniSessionEvent;

/// How long the view waits for the user before giving up (the question is
/// then answered with a cancellation).
pub const QUESTION_TIMEOUT_SECS: u64 = 600;

/// Domain-side interaction handler: forward follow-up questions to the mini
/// event loop. `on_tool_approval_requested` is a no-op by design (approvals
/// flow through `MiniApprovalHandler`).
pub struct MiniInteractionHandler {
    tx: UnboundedSender<MiniSessionEvent>,
}

impl MiniInteractionHandler {
    pub fn new(tx: UnboundedSender<MiniSessionEvent>) -> Self {
        Self { tx }
    }
}

impl UserInteractionHandler for MiniInteractionHandler {
    fn on_interaction(&self, _record: &AgentUserInteractionEventRecord) {}

    fn on_tool_approval_requested(&self, _execution_id: &str, _request: &Value) {
        // Approvals go through the dedicated `MiniApprovalHandler` channel.
    }

    fn on_followup_question_requested(&self, _execution_id: &str, request: &Value) {
        let interaction_id = request
            .get("interactionId")
            .or_else(|| request.get("interaction_id"))
            .or_else(|| request.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // The UI owns the reply; a closed channel only means the session is
        // already tearing down.
        let _ = self.tx.send(MiniSessionEvent::QuestionRequested {
            interaction_id,
            request: request.clone(),
        });
    }
}

/// One selectable option of a question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
}

/// The shape of the answer the view produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionOutcome {
    /// One or more picked option indexes (single-select keeps the last pick).
    Selected(Vec<usize>),
    /// Free-form answer (custom input path).
    Custom(String),
    /// Dismissed without an answer.
    Cancelled,
}

/// Pure question view state: the parsed request, the toggled picks and the
/// rendering / decision mapping. Owned by the footer while
/// `FooterView::Question` is active.
#[derive(Debug, Clone)]
pub struct QuestionView {
    interaction_id: String,
    prompt: String,
    options: Vec<QuestionOption>,
    /// Whether more than one option may be picked.
    multi: bool,
    /// Whether a free-form answer is accepted (Enter with no picks).
    allow_custom: bool,
    /// Toggled option indexes (ordered by toggle time).
    picked: Vec<usize>,
}

impl QuestionView {
    /// Parse a question view from the interaction request payload.
    ///
    /// Accepted shapes (defensive — the request is a raw JSON value):
    /// `{"prompt": "...", "options": ["a", "b"], "multi": true,
    /// "allowCustom": true}` and the workflow `USER_INTERACTION` shape
    /// (`{"question": "...", "choices": [...]}`). Anything missing degrades
    /// to a prompt-only question answered through the custom path.
    pub fn from_request(interaction_id: impl Into<String>, request: &Value) -> Self {
        let prompt = request
            .get("prompt")
            .or_else(|| request.get("question"))
            .and_then(Value::as_str)
            .unwrap_or("follow-up question")
            .to_string();
        let raw_options = request
            .get("options")
            .or_else(|| request.get("choices"))
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let options: Vec<QuestionOption> = match raw_options {
            Value::Array(items) => items
                .into_iter()
                .filter_map(|item| match item {
                    Value::String(label) => Some(QuestionOption { label }),
                    Value::Object(map) => map
                        .get("label")
                        .or_else(|| map.get("text"))
                        .or_else(|| map.get("value"))
                        .and_then(Value::as_str)
                        .map(|label| QuestionOption {
                            label: label.to_string(),
                        }),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let multi = request
            .get("multi")
            .or_else(|| request.get("multiple"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let allow_custom = request
            .get("allowCustom")
            .or_else(|| request.get("allow_custom"))
            .or_else(|| request.get("custom"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        Self {
            interaction_id: interaction_id.into(),
            prompt,
            options,
            multi,
            allow_custom,
            picked: Vec::new(),
        }
    }

    /// The interaction this question belongs to (may be empty when the
    /// request carried no id — demo questions).
    pub fn interaction_id(&self) -> &str {
        &self.interaction_id
    }

    /// The question prompt.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// The offered options.
    pub fn options(&self) -> &[QuestionOption] {
        &self.options
    }

    /// Toggle the n-th option (1-based, matching the `Pick(n)` keymap
    /// action). Single-select replaces the previous pick. Returns whether
    /// anything changed.
    pub fn pick(&mut self, n: u8) -> bool {
        let index = usize::from(n.wrapping_sub(1));
        if index >= self.options.len() {
            return false;
        }
        if let Some(pos) = self.picked.iter().position(|p| *p == index) {
            self.picked.remove(pos);
        } else if self.multi {
            self.picked.push(index);
        } else {
            self.picked.clear();
            self.picked.push(index);
        }
        true
    }

    /// Currently picked option indexes.
    pub fn picked(&self) -> &[usize] {
        &self.picked
    }

    /// Resolve the view into an answer outcome. Enter with picks confirms
    /// them; Enter with no picks is a custom (empty) answer when custom
    /// input is allowed, otherwise a cancellation.
    pub fn submit(&self) -> QuestionOutcome {
        if !self.picked.is_empty() {
            QuestionOutcome::Selected(self.picked.clone())
        } else if self.allow_custom {
            QuestionOutcome::Custom(String::new())
        } else {
            QuestionOutcome::Cancelled
        }
    }

    /// A cancelled outcome (Esc).
    pub fn cancel(&self) -> QuestionOutcome {
        QuestionOutcome::Cancelled
    }

    /// The `response_data` payload sent back through
    /// `respond_interaction` for an outcome.
    pub fn response_value(&self, outcome: &QuestionOutcome) -> Value {
        match outcome {
            QuestionOutcome::Selected(indexes) => {
                let labels: Vec<String> = indexes
                    .iter()
                    .filter_map(|i| self.options.get(*i))
                    .map(|o| o.label.clone())
                    .collect();
                if self.multi || labels.len() != 1 {
                    Value::Array(labels.into_iter().map(Value::String).collect())
                } else {
                    Value::String(labels.into_iter().next().unwrap_or_default())
                }
            }
            QuestionOutcome::Custom(text) => Value::String(text.clone()),
            QuestionOutcome::Cancelled => Value::Null,
        }
    }

    /// Short human-readable answer for the scrollback echo.
    pub fn answer_text(&self, outcome: &QuestionOutcome) -> String {
        match outcome {
            QuestionOutcome::Selected(indexes) => indexes
                .iter()
                .filter_map(|i| self.options.get(*i))
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            QuestionOutcome::Custom(text) => text.clone(),
            QuestionOutcome::Cancelled => "(cancelled)".to_string(),
        }
    }

    /// Key hints line.
    pub fn hints(&self) -> String {
        let mode = if self.multi { "multi-select" } else { "select" };
        format!("1-9 pick ({mode}) · Enter confirm · Esc cancel")
    }

    /// Render the view rows for the given width (window height bounded by
    /// the caller).
    pub fn render_lines(&self, width: usize) -> Vec<ratatui::text::Line<'static>> {
        use ratatui::text::{Line, Span};
        let mut lines = Vec::new();
        lines.push(Line::from(Span::raw(format!("? {}", self.prompt))));
        for (i, option) in self.options.iter().enumerate() {
            let marker = if self.picked.contains(&i) {
                if self.multi {
                    "[x]"
                } else {
                    "(x)"
                }
            } else if self.multi {
                "[ ]"
            } else {
                "( )"
            };
            let label = format!(" {}. {marker} {}", i + 1, option.label);
            lines.push(Line::from(Span::raw(truncate(&label, width))));
        }
        if self.options.is_empty() {
            lines.push(Line::from(Span::raw(
                " (no options — answer with Enter for a custom reply)",
            )));
        }
        lines.push(Line::from(Span::raw(truncate(&self.hints(), width))));
        lines
    }
}

/// Truncate to `width` columns on a grapheme boundary.
fn truncate(text: &str, width: usize) -> String {
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

/// Map a keymap action (Question context) onto a view transition outcome.
/// `Pick` toggles are handled by the view itself; this covers the
/// confirm/cancel keys.
pub fn outcome_for_action(action: KeyAction) -> Option<QuestionOutcome> {
    match action {
        KeyAction::Select | KeyAction::Submit => Some(QuestionOutcome::Selected(Vec::new())),
        KeyAction::Cancel | KeyAction::Back => Some(QuestionOutcome::Cancelled),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view(request: Value) -> QuestionView {
        QuestionView::from_request("ui-1", &request)
    }

    #[test]
    fn parses_prompt_options_and_flags() {
        let v = view(json!({
            "prompt": "pick one",
            "options": ["red", "green", "blue"],
            "multi": false,
            "allowCustom": false,
        }));
        assert_eq!(v.prompt(), "pick one");
        assert_eq!(v.options().len(), 3);
        assert!(!v.multi);
        assert!(!v.allow_custom);
    }

    #[test]
    fn parses_workflow_shapes_and_degrades() {
        let v = view(json!({ "question": "continue?", "choices": ["yes", "no"] }));
        assert_eq!(v.prompt(), "continue?");
        assert_eq!(v.options().len(), 2);

        let bare = view(json!({}));
        assert_eq!(bare.prompt(), "follow-up question");
        assert!(bare.options().is_empty());
        assert!(bare.allow_custom, "custom input is the default fallback");
    }

    #[test]
    fn single_select_replaces_the_pick() {
        let mut v = view(json!({ "options": ["a", "b", "c"] }));
        assert!(v.pick(2));
        assert_eq!(v.picked(), &[1]);
        assert!(v.pick(3));
        assert_eq!(v.picked(), &[2], "single select replaces");
        assert!(!v.pick(9), "out of range ignored");
        assert!(v.pick(3), "toggle off");
        assert!(v.picked().is_empty());
    }

    #[test]
    fn multi_select_accumulates() {
        let mut v = view(json!({ "options": ["a", "b"], "multi": true }));
        v.pick(1);
        v.pick(2);
        assert_eq!(v.picked(), &[0, 1]);
    }

    #[test]
    fn submit_resolves_picks_custom_and_cancel() {
        let mut v = view(json!({ "options": ["a", "b"] }));
        v.pick(2);
        match v.submit() {
            QuestionOutcome::Selected(idx) => assert_eq!(idx, vec![1]),
            other => panic!("{other:?}"),
        }
        // No picks + custom allowed → custom (empty) answer.
        let mut v2 = view(json!({ "options": ["a"], "allowCustom": true }));
        v2.pick(1);
        v2.pick(1); // toggle off
        assert_eq!(v2.submit(), QuestionOutcome::Custom(String::new()));
        // No picks + custom not allowed → cancelled.
        let v3 = view(json!({ "options": ["a"], "allowCustom": false }));
        assert_eq!(v3.submit(), QuestionOutcome::Cancelled);
    }

    #[test]
    fn response_value_encodes_single_multi_and_custom() {
        let mut v = view(json!({ "options": ["a", "b"] }));
        v.pick(1);
        assert_eq!(
            v.response_value(&v.submit()),
            json!("a"),
            "single pick becomes a bare string"
        );

        let mut m = view(json!({ "options": ["a", "b"], "multi": true }));
        m.pick(1);
        m.pick(2);
        assert_eq!(m.response_value(&m.submit()), json!(["a", "b"]));

        let c = view(json!({}));
        assert_eq!(
            c.response_value(&QuestionOutcome::Custom("typed".into())),
            json!("typed")
        );
        assert_eq!(c.response_value(&QuestionOutcome::Cancelled), Value::Null);
    }

    #[test]
    fn answer_text_and_render_lines() {
        let mut v = view(json!({ "prompt": "p", "options": ["a", "b"] }));
        v.pick(2);
        assert_eq!(v.answer_text(&v.submit()), "b");
        assert_eq!(v.answer_text(&QuestionOutcome::Cancelled), "(cancelled)");
        let lines = v.render_lines(80);
        assert!(lines[0].to_string().contains("? p"));
        assert!(lines[1].to_string().contains("1. ( ) a"));
        assert!(lines[2].to_string().contains("2. (x) b"));
        assert!(lines.last().unwrap().to_string().contains("Esc cancel"));
    }

    #[test]
    fn outcome_for_action_maps_confirm_and_cancel() {
        assert_eq!(
            outcome_for_action(KeyAction::Select),
            Some(QuestionOutcome::Selected(Vec::new()))
        );
        assert_eq!(
            outcome_for_action(KeyAction::Cancel),
            Some(QuestionOutcome::Cancelled)
        );
        assert_eq!(outcome_for_action(KeyAction::Help), None);
    }

    #[tokio::test]
    async fn handler_forwards_questions_to_the_event_loop() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let handler = MiniInteractionHandler::new(tx);
        handler.on_followup_question_requested(
            "exec-1",
            &json!({ "interactionId": "ui-9", "prompt": "q" }),
        );
        match rx.recv().await {
            Some(MiniSessionEvent::QuestionRequested {
                interaction_id,
                request,
            }) => {
                assert_eq!(interaction_id, "ui-9");
                assert_eq!(request["prompt"], "q");
            }
            other => panic!("expected a question event, got {other:?}"),
        }
        // Approval requests must not surface here.
        handler.on_tool_approval_requested("exec-1", &json!({ "tool": "bash" }));
        assert!(rx.try_recv().is_err());
    }
}

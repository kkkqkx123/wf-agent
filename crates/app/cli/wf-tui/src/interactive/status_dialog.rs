//! Interactive status and dialogs: pacer, approvals and blockers.

use super::*;
use crate::animation::AnimationController;
use crate::approval_overlay::ApprovalRemembered;
use crate::context_sidebar::{ContextSnapshot, SidebarSnapshot, TodoLine};
use crate::footer::Footer;
use crate::pager::ReplayPager;
use crate::prep_cache::PreparedScrollback;
use crate::reducer::Phase;
use crate::reducer::SessionReducer;
use crate::stream_pacer::SegmentedPacer;
use crate::terminal::{DoublePressTracker, ESC_DOUBLE_PRESS_WINDOW, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::transcript::HistoryLine;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use wf_client::WfClient;
use wf_types::events::ExecutionStreamEvent;
use wf_types::interaction::tool_approval::ToolApprovalRequestData;

impl InteractiveController {
    /// Owned read-only snapshot of the render state for headless replay and
    /// baseline reports. The production draw path still owns the cache; this
    /// is the convergence step toward drawing from the view alone.
    pub fn snapshot_view(&self) -> crate::render_model::TestRenderModel {
        let mut view = crate::render_model::TestRenderModel::new(self.last_layout_width.max(1));
        for line in &self.scrollback {
            let text: String = line
                .text()
                .lines
                .iter()
                .flat_map(|row| row.spans.iter().map(|span| span.content.as_ref()))
                .collect::<Vec<_>>()
                .join("\n");
            view.history.push(text);
        }
        view.version = self.content_version;
        view.streaming = self.stream.streaming_text().to_string();
        view.scroll = self.view_scroll;
        view.height = self.last_layout_height.max(1);
        view.overlay = self.footer.approval.is_some();
        view.now_ms = self.now_ms();
        view.footer = crate::redraw::footer_digest(&self.footer.state);
        view
    }

    /// Queued but not yet visible pacer bytes.
    pub fn pacer_pending_len(&self) -> usize {
        self.pacer.pending_len()
    }

    /// Disable pacing while keeping the interface (rollback escape hatch).
    pub fn set_pacer_bypass(&mut self, bypass: bool) {
        self.pacer.set_bypass(bypass);
    }

    /// Visible answer-text prefix length in bytes.
    pub fn pacer_visible_len(&self) -> usize {
        self.pacer.text_visible_len()
    }

    /// Whether tail-follow catch-up still lags behind the live tail.
    pub fn tail_catchup_active(&self) -> bool {
        self.tail_lag > 0
    }

    /// Whether a turn is actively processing.
    pub fn is_processing(&self) -> bool {
        self.footer.state.phase == Phase::Streaming
    }

    /// Whether any streaming text is currently visible.
    pub fn streaming_empty(&self) -> bool {
        self.stream.streaming_text().is_empty()
    }

    /// Whether recorded animation cells exist for a cheap tick.
    pub fn has_anim_cells(&self) -> bool {
        !self.anim_rows.is_empty()
    }

    /// Whether the composer holds any input text.
    pub fn composer_empty(&self) -> bool {
        self.footer.composer.is_empty()
    }

    /// Take a pending content-based title, if any.
    pub fn take_pending_title(&mut self) -> Option<String> {
        self.pending_title.take()
    }

    /// Whether an approval overlay forces a full frame.
    pub fn approval_active(&self) -> bool {
        self.footer.approval.is_some() || self.pending_approval.is_some()
    }

    /// Whether a host-mediated plugin dialog is on screen. While set the
    /// host owns focus and the composer stays blocked, mirroring the
    /// approval/question blockers.
    pub fn plugin_dialog_active(&self) -> bool {
        self.plugin_dialog.is_some()
    }

    /// Open one host-mediated plugin dialog through the existing question
    /// float. Rejects empty selects before presenting and refuses when
    /// another blocker owns focus; the host keeps dismissal ownership.
    /// `now_ms` plus `timeout_secs` form the fail-closed deadline.
    pub fn open_plugin_dialog(
        &mut self,
        plugin_id: impl Into<String>,
        request: tui_core::dialog::DialogRequest,
        now_ms: u64,
        timeout_secs: u64,
    ) -> Result<(), String> {
        if self.footer.approval.is_some() || self.footer.question.is_some() {
            return Err("another dialog owns focus".to_string());
        }
        if let tui_core::dialog::DialogRequest::Select { options, .. } = &request {
            if options.is_empty() {
                return Err("select dialog needs at least one option".to_string());
            }
        }
        let payload = plugin_dialog_question_payload(&request);
        let view = crate::question_overlay::QuestionView::from_request(
            format!("plugin:{}", plugin_id.into()),
            &payload,
        );
        self.footer.question = Some(view);
        self.plugin_dialog = Some(PluginDialogPending {
            request,
            deadline_ms: now_ms.saturating_add(timeout_secs.max(1).saturating_mul(1000)),
        });
        Ok(())
    }

    /// Answer the open plugin dialog from the question float outcome and
    /// release host focus. Returns `None` when no dialog is open.
    pub fn answer_plugin_dialog(
        &mut self,
        outcome: crate::question_overlay::QuestionOutcome,
    ) -> Option<tui_core::dialog::DialogOutcome> {
        let pending = self.plugin_dialog.take()?;
        self.footer.question = None;
        Some(answer_plugin_dialog_from_question(
            &pending.request,
            outcome,
        ))
    }

    /// Fail closed when the open dialog passes its deadline. Returns the
    /// closed outcome when a timeout fired, else `None`.
    pub fn cancel_expired_plugin_dialog(
        &mut self,
        now_ms: u64,
    ) -> Option<tui_core::dialog::DialogOutcome> {
        let expired = self
            .plugin_dialog
            .as_ref()
            .is_some_and(|pending| now_ms >= pending.deadline_ms);
        if !expired {
            return None;
        }
        let pending = self.plugin_dialog.take()?;
        self.footer.question = None;
        Some(tui_core::dialog::answer_headless(&pending.request, None))
    }

    /// Dismiss the open plugin dialog without an answer (host-owned close).
    pub fn close_plugin_dialog(&mut self) -> bool {
        if self.plugin_dialog.is_none() {
            return false;
        }
        self.plugin_dialog = None;
        self.footer.question = None;
        true
    }

    /// Record a newly opened blocker dialog. Reopening a source replaces
    /// its position instead of stacking a second entry.
    pub(crate) fn note_blocker_opened(
        &mut self,
        kind: tui_terminal::terminal::state::ProgramStatusKind,
    ) {
        self.blocker_order.retain(|existing| *existing != kind);
        self.blocker_order.push(kind);
    }

    /// Most recently opened blocker whose dialog is still visible.
    /// Cleared dialogs are skipped so a missed removal can never stick
    /// the report on a closed dialog.
    pub(crate) fn open_blocker(&self) -> Option<tui_terminal::terminal::state::ProgramStatusKind> {
        use tui_terminal::terminal::state::ProgramStatusKind;
        self.blocker_order.iter().rev().copied().find(|kind| {
            matches!(
                kind,
                ProgramStatusKind::Permission if self.footer.approval.is_some()
            ) || matches!(
                kind,
                ProgramStatusKind::Question if self.footer.question.is_some()
            )
        })
    }

    /// Desired terminal program status for this session. A blocked dialog
    /// wins over the run state and the most recently opened still-open
    /// dialog wins over older ones. The running and finished states carry
    /// the session title and the question state carries the question
    /// prompt so host terminals can show them; error text stays a single
    /// first line and prompts never leave the TUI.
    pub fn terminal_program_status(
        &self,
        session_title: Option<&str>,
    ) -> tui_terminal::terminal::state::ProgramStatus {
        use tui_terminal::terminal::state::{ProgramStatus, ProgramStatusKind, ProgramStatusState};
        if let Some(blocked) = self.open_blocker() {
            let message = match blocked {
                ProgramStatusKind::Permission => self
                    .footer
                    .approval
                    .as_ref()
                    .map(|approval| approval.request().tool_name.clone()),
                ProgramStatusKind::Question => self.footer.question.as_ref().map(|question| {
                    let prompt = question.prompt().lines().next().unwrap_or("").trim();
                    if prompt.is_empty() {
                        "Question".to_string()
                    } else {
                        prompt.to_string()
                    }
                }),
                ProgramStatusKind::Auth => None,
            };
            if let Some(message) = message {
                return ProgramStatus {
                    state: ProgramStatusState::Blocked,
                    app: Some("wf".to_string()),
                    kind: Some(blocked),
                    message: Some(message),
                };
            }
        }
        if self.is_processing() {
            return ProgramStatus {
                state: ProgramStatusState::Working,
                app: Some("wf".to_string()),
                kind: None,
                message: session_title.map(str::to_string),
            };
        }
        match self.terminal_program_status {
            ProgramStatusState::Error => ProgramStatus {
                state: ProgramStatusState::Error,
                app: Some("wf".to_string()),
                kind: None,
                message: self.terminal_program_error.clone(),
            },
            ProgramStatusState::Done => ProgramStatus {
                state: ProgramStatusState::Done,
                app: Some("wf".to_string()),
                kind: None,
                message: session_title.map(str::to_string),
            },
            _ => ProgramStatus {
                state: ProgramStatusState::Idle,
                app: Some("wf".to_string()),
                kind: None,
                message: None,
            },
        }
    }

    /// Single-cell fast-path inputs for the current state. The shell supplies
    /// the overlay flag; input staleness is already covered because any input
    /// change grades a full frame before this is consulted.
    pub fn single_cell_inputs(&self, overlay_full: bool) -> crate::redraw::SingleCellInputs {
        crate::redraw::SingleCellInputs {
            processing: self.is_processing(),
            primary_spinner: self.spinner_enabled,
            streaming_empty: self.streaming_empty(),
            centered: false,
            mouse_scroll: false,
            remote_startup: false,
            overlay_full,
            input_changed: false,
            animation_cells: self.has_anim_cells(),
        }
    }
}

/// One host-mediated plugin dialog behind the question float. The host
/// owns focus and dismissal; the plugin only sees the mapped outcome.
#[derive(Debug, Clone)]
pub(crate) struct PluginDialogPending {
    request: tui_core::dialog::DialogRequest,
    deadline_ms: u64,
}

/// Build the question-float payload for one dialog request. Confirm maps
/// to a Yes/No pick, select keeps its options, input and edit open the
/// free-form path (edit embeds its initial text in the prompt).
pub(crate) fn plugin_dialog_question_payload(
    request: &tui_core::dialog::DialogRequest,
) -> serde_json::Value {
    match request {
        tui_core::dialog::DialogRequest::Confirm { title, message, .. } => serde_json::json!({
            "prompt": format!("{title}: {message}"),
            "options": ["Yes", "No"],
            "allowCustom": false,
        }),
        tui_core::dialog::DialogRequest::Select {
            title,
            message,
            options,
        } => serde_json::json!({
            "prompt": format!("{title}: {message}"),
            "options": options,
            "allowCustom": false,
        }),
        tui_core::dialog::DialogRequest::Input {
            title,
            message,
            placeholder,
            ..
        } => serde_json::json!({
            "prompt": format!("{title}: {message}"),
            "options": [],
            "allowCustom": true,
            "placeholder": placeholder,
        }),
        tui_core::dialog::DialogRequest::Edit {
            title,
            message,
            initial,
            ..
        } => serde_json::json!({
            "prompt": format!("{title}: {message} (initial: {initial})"),
            "options": [],
            "allowCustom": true,
        }),
    }
}

/// Map a question-float outcome back onto the dialog contract. Confirm
/// approves only on the first option; select resolves the picked index;
/// input and edit take the custom text, with blank edit text dismissing.
pub(crate) fn answer_plugin_dialog_from_question(
    request: &tui_core::dialog::DialogRequest,
    outcome: crate::question_overlay::QuestionOutcome,
) -> tui_core::dialog::DialogOutcome {
    use crate::question_overlay::QuestionOutcome as Q;
    use tui_core::dialog::DialogOutcome as D;
    match request {
        tui_core::dialog::DialogRequest::Confirm { .. } => match outcome {
            Q::Selected(picks) => D::Confirmed(picks.contains(&0)),
            Q::Custom(text) => D::Confirmed(tui_core::dialog::parse_confirm_answer(&text)),
            Q::Cancelled => D::Confirmed(false),
        },
        tui_core::dialog::DialogRequest::Select { options, .. } => match outcome {
            Q::Selected(mut picks) => {
                let first = picks.pop();
                D::Selected(first.filter(|index| *index < options.len()))
            }
            Q::Custom(text) => {
                D::Selected(tui_core::dialog::parse_select_answer(&text, options.len()))
            }
            Q::Cancelled => D::Selected(None),
        },
        tui_core::dialog::DialogRequest::Input { .. } => match outcome {
            Q::Selected(_) => D::Entered(None),
            Q::Custom(text) => D::Entered(Some(text)),
            Q::Cancelled => D::Cancelled,
        },
        tui_core::dialog::DialogRequest::Edit { .. } => match outcome {
            Q::Selected(_) => D::Edited(None),
            Q::Custom(text) if text.trim().is_empty() => D::Edited(None),
            Q::Custom(text) => D::Edited(Some(text)),
            Q::Cancelled => D::Cancelled,
        },
    }
}

/// Placeholder number inside `[Image N]` or `[File N]` labels.
pub(crate) fn parse_placeholder_index(label: &str, tag: &str) -> Option<usize> {
    let inner = label.strip_prefix('[')?.strip_suffix(']')?;
    let (kind, number) = inner.split_once(' ')?;
    if kind != tag {
        return None;
    }
    let index: usize = number.trim().parse().ok()?;
    (index > 0).then_some(index)
}

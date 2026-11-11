use super::*;
use tui_core::panel_select::Motion;

async fn started_controller() -> InteractiveController {
    let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
    InteractiveController::start(client, "exec".into()).await
}

async fn seeded_controller(width: u16, height: u16, lines: &[&str]) -> InteractiveController {
    let mut ctl = started_controller().await;
    ctl.last_layout_width = width;
    ctl.last_layout_height = height;
    ctl.last_scroll_area = Some(ratatui::layout::Rect::new(0, 0, width, height));
    for line in lines {
        ctl.pending_scroll.push(HistoryLine::new(*line));
    }
    ctl.settle_scrollback();
    ctl.tail_lag = 0;
    ctl.view_scroll = 0;
    ctl.sync_transcript_layout();
    ctl
}

fn selection_at(
    ctl: &InteractiveController,
    start: (usize, usize),
    end: (usize, usize),
) -> TranscriptSelection {
    let anchor = ctl
        .transcript_pos_at_display_row(start.0, start.1)
        .expect("anchor row is inside the viewport");
    let head = ctl
        .transcript_pos_at_display_row(end.0, end.1)
        .expect("head row is inside the viewport");
    TranscriptSelection { anchor, head }
}

#[tokio::test]
async fn scrolled_selection_keeps_the_same_characters() {
    let mut ctl = seeded_controller(20, 3, &["line0", "line1", "line2", "line3", "line4"]).await;
    let selection = selection_at(&ctl, (0, 0), (0, 5));
    assert_eq!(
        ctl.selected_transcript_text(selection),
        Some("line2".into())
    );
    ctl.view_scroll = 2;
    assert_eq!(
        ctl.selected_transcript_text(selection),
        Some("line2".into())
    );
    let moved = ctl
        .transcript_pos_at_display_row(2, 0)
        .expect("the same row is visible after scrolling");
    assert_eq!(moved, selection.anchor);
}

#[tokio::test]
async fn resized_selection_keeps_the_same_characters() {
    let mut ctl = seeded_controller(5, 3, &["abcdefghij"]).await;
    let selection = selection_at(&ctl, (0, 1), (1, 2));
    assert_eq!(
        ctl.selected_transcript_text(selection),
        Some("bcdefgh".into())
    );
    ctl.last_layout_width = 10;
    assert_eq!(
        ctl.selected_transcript_text(selection),
        Some("bcdefgh".into())
    );
    let moved = ctl
        .transcript_pos_at_display_row(0, 7)
        .expect("the logical position maps to one row at the wider width");
    assert_eq!(moved, selection.head);
}

#[tokio::test]
async fn autoscroll_moves_the_head_without_drifting_the_anchor() {
    let mut ctl = seeded_controller(
        20,
        3,
        &[
            "row0", "row1", "row2", "row3", "row4", "row5", "row6", "row7",
        ],
    )
    .await;
    let selection = selection_at(&ctl, (0, 0), (2, 3));
    ctl.set_transcript_selection(selection);
    ctl.start_transcript_autoscroll(1);
    assert!(ctl.tick_transcript_autoscroll());
    let after = ctl
        .transcript_selection()
        .expect("autoscroll keeps the selection alive");
    assert_eq!(after.anchor, selection.anchor);
    assert_ne!(after.head, selection.head);
    assert_eq!(
        ctl.selected_transcript_text(after),
        Some("row5\nrow6\nrow7".into())
    );
}

#[tokio::test]
async fn streaming_tail_stays_out_of_the_selection() {
    let mut ctl = seeded_controller(20, 4, &["committed"]).await;
    ctl.streaming = Some(HistoryLine::new("unfinished"));
    ctl.sync_transcript_layout();
    assert!(ctl.select_all_transcript());
    let selection = ctl.transcript_selection().expect("selection exists");
    assert_eq!(
        ctl.selected_transcript_text(selection),
        Some("committed".into())
    );
}

#[tokio::test]
async fn keyboard_selection_works_without_mouse_capture() {
    let mut ctl = seeded_controller(20, 3, &["alpha", "beta", "gamma"]).await;
    assert!(ctl.transcript_selection_keys_active());
    assert!(ctl.extend_transcript_selection(Motion::Up));
    let selection = ctl
        .transcript_selection()
        .expect("keyboard selection starts");
    assert_eq!(
        ctl.selected_transcript_text(selection),
        Some("beta\ng".into())
    );
    assert!(ctl.extend_transcript_selection(Motion::Up));
    let grown = ctl.transcript_selection().expect("selection stays");
    assert_eq!(
        ctl.selected_transcript_text(grown),
        Some("alpha\nbeta\ng".into())
    );
}

#[tokio::test]
async fn markdown_selection_uses_the_rendered_projection() {
    let mut ctl = started_controller().await;
    ctl.last_layout_width = 40;
    ctl.last_layout_height = 4;
    ctl.last_scroll_area = Some(ratatui::layout::Rect::new(0, 0, 40, 4));
    ctl.pending_scroll
        .push(HistoryLine::markdown("**bold** text"));
    ctl.settle_scrollback();
    ctl.tail_lag = 0;
    ctl.view_scroll = 0;
    ctl.sync_transcript_layout();
    assert!(ctl.select_all_transcript());
    let selection = ctl.transcript_selection().expect("selection exists");
    assert_eq!(
        ctl.selected_transcript_text(selection),
        Some("bold text".into())
    );
}

#[tokio::test]
async fn additive_regions_copy_together() {
    let mut ctl = seeded_controller(20, 4, &["alpha", "beta", "gamma", "delta"]).await;
    let first = selection_at(&ctl, (0, 0), (0, 5));
    ctl.set_transcript_selection(first);
    ctl.begin_additive_transcript_selection();
    let second = selection_at(&ctl, (2, 0), (2, 5));
    ctl.set_transcript_selection(second);
    assert_eq!(
        ctl.selected_transcript_text_all(None),
        Some("alpha\ngamma".into())
    );
    assert!(ctl.copy_transcript_selection());
    assert!(ctl.take_pending_copy().is_some());
    assert!(ctl.has_transcript_selection());
}

#[tokio::test]
async fn tail_catchup_slides_large_appends_in_bounded_steps() {
    let mut ctl = started_controller().await;
    ctl.simplified_render = false;
    ctl.tail_lag = 10;
    let first = ctl.advance_tail_catchup(10);
    assert_eq!(first, 7, "one frame advances at most three rows");
    assert_eq!(ctl.tail_lag, 7);
    // Fixed append sequence follows the curve without overshooting.
    let mut offsets = vec![first];
    while ctl.tail_lag > 0 {
        offsets.push(ctl.advance_tail_catchup(10));
    }
    assert_eq!(*offsets.last().unwrap_or(&1), 0);
    assert!(offsets.len() > 2, "large lags must span frames");
    for pair in offsets.windows(2) {
        assert!(pair[0] - pair[1] <= crate::tail::TAIL_CATCHUP_MAX_STEP);
    }
}

#[tokio::test]
async fn explicit_navigation_snaps_tail_instantly() {
    let mut ctl = started_controller().await;
    ctl.simplified_render = false;
    ctl.tail_lag = 9;
    ctl.jump_to_latest();
    assert_eq!(ctl.tail_lag, 0);
    assert_eq!(ctl.advance_tail_catchup(10), 0);
}

#[tokio::test]
async fn static_rendering_snaps_catchup_instantly() {
    let mut ctl = started_controller().await;
    ctl.simplified_render = true;
    ctl.tail_lag = 9;
    assert_eq!(ctl.advance_tail_catchup(10), 0);
}

#[tokio::test]
async fn manual_scroll_abandons_catchup_softly() {
    let mut ctl = started_controller().await;
    ctl.tail_lag = 6;
    ctl.scroll_history_up();
    assert_eq!(ctl.tail_lag, 0);
    assert!(ctl.view_scroll > 0);
}

fn approval_view(tool_name: &str) -> crate::approval_overlay::ApprovalView {
    crate::approval_overlay::ApprovalView::new(
        wf_types::interaction::tool_approval::ToolApprovalRequestData {
            tool_call_id: "call-1".to_string(),
            tool_name: tool_name.to_string(),
            tool_description: None,
            parameters: serde_json::Value::Null,
            risk_level: None,
            pending_queue: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
        },
    )
}

fn question_view() -> crate::question_overlay::QuestionView {
    crate::question_overlay::QuestionView::from_request(
        "q-1",
        &serde_json::json!({ "prompt": "Pick a target\nsecond line" }),
    )
}

#[tokio::test]
async fn blocker_reports_the_most_recently_opened_dialog() {
    use tui_terminal::terminal::state::ProgramStatusKind;
    let mut ctl = started_controller().await;
    ctl.footer.approval = Some(approval_view("shell"));
    ctl.note_blocker_opened(ProgramStatusKind::Permission);
    ctl.footer.question = Some(question_view());
    ctl.note_blocker_opened(ProgramStatusKind::Question);
    let status = ctl.terminal_program_status(None);
    assert_eq!(status.kind, Some(ProgramStatusKind::Question));
    assert_eq!(status.message.as_deref(), Some("Pick a target"));
}

#[tokio::test]
async fn blocker_falls_back_when_the_top_dialog_closes() {
    use tui_terminal::terminal::state::ProgramStatusKind;
    let mut ctl = started_controller().await;
    ctl.footer.approval = Some(approval_view("shell"));
    ctl.note_blocker_opened(ProgramStatusKind::Permission);
    ctl.footer.question = Some(question_view());
    ctl.note_blocker_opened(ProgramStatusKind::Question);
    ctl.footer.question = None;
    let status = ctl.terminal_program_status(None);
    assert_eq!(status.kind, Some(ProgramStatusKind::Permission));
    assert_eq!(status.message.as_deref(), Some("shell"));
}

#[tokio::test]
async fn blocker_reopen_replaces_instead_of_stacking() {
    use tui_terminal::terminal::state::ProgramStatusKind;
    let mut ctl = started_controller().await;
    ctl.footer.approval = Some(approval_view("shell"));
    ctl.note_blocker_opened(ProgramStatusKind::Permission);
    ctl.footer.question = Some(question_view());
    ctl.note_blocker_opened(ProgramStatusKind::Question);
    ctl.note_blocker_opened(ProgramStatusKind::Permission);
    let status = ctl.terminal_program_status(None);
    assert_eq!(status.kind, Some(ProgramStatusKind::Permission));
    ctl.footer.approval = None;
    ctl.footer.question = None;
    let idle = ctl.terminal_program_status(None);
    assert_eq!(
        idle.state,
        tui_terminal::terminal::state::ProgramStatusState::Idle
    );
}

// ── graceful-exit error suppression ─────────────────────────────────

#[test]
fn terminal_events_render_normally_when_active() {
    let failed = ExecutionStreamEvent::Failed {
        error: "boom".into(),
    };
    let interrupted = ExecutionStreamEvent::Interrupted {
        reason: "user".into(),
    };
    assert!(InteractiveController::should_render_terminal(
        false, &failed
    ));
    assert!(InteractiveController::should_render_terminal(
        false,
        &interrupted
    ));
}

#[tokio::test]
async fn plugin_dialog_owns_focus_and_fails_closed_on_timeout() {
    use crate::question_overlay::QuestionOutcome;
    let mut ctl = started_controller().await;
    let confirm = tui_core::dialog::DialogRequest::Confirm {
        title: "rm?".to_string(),
        message: "delete?".to_string(),
        danger: true,
    };
    ctl.open_plugin_dialog("review.bot", confirm, 1000, 5)
        .expect("opens");
    assert!(ctl.plugin_dialog_active());
    assert!(ctl.footer.question.is_some());
    let denied = ctl
        .open_plugin_dialog(
            "other",
            tui_core::dialog::DialogRequest::Input {
                title: "x".to_string(),
                message: "y".to_string(),
                placeholder: None,
                secret: false,
            },
            1000,
            5,
        )
        .is_err();
    assert!(denied, "second dialog cannot steal focus");
    let outcome = ctl.answer_plugin_dialog(QuestionOutcome::Selected(vec![0]));
    assert_eq!(
        outcome,
        Some(tui_core::dialog::DialogOutcome::Confirmed(true))
    );
    assert!(!ctl.plugin_dialog_active());
    let select = tui_core::dialog::DialogRequest::Select {
        title: "pick".to_string(),
        message: "one".to_string(),
        options: vec!["a".to_string(), "b".to_string()],
    };
    ctl.open_plugin_dialog("review.bot", select, 1000, 1)
        .expect("opens");
    let timed_out = ctl.cancel_expired_plugin_dialog(1000 + 1001);
    assert_eq!(
        timed_out,
        Some(tui_core::dialog::DialogOutcome::Selected(None))
    );
    assert!(!ctl.plugin_dialog_active());
}

#[test]
fn terminal_failures_are_suppressed_after_graceful_exit() {
    // A `Failed`/`Interrupted` event that arrives while the session is
    // exiting must not land on screen (or be treated as a failure the UI
    // needs to reflect); the graceful flag routes it into the quiet drain.
    let failed = ExecutionStreamEvent::Failed {
        error: "runtime closed".into(),
    };
    let interrupted = ExecutionStreamEvent::Interrupted {
        reason: "shutdown".into(),
    };
    assert!(!InteractiveController::should_render_terminal(
        true, &failed
    ));
    assert!(!InteractiveController::should_render_terminal(
        true,
        &interrupted
    ));
    // Non-terminal events are unaffected by the flag.
    let llm = ExecutionStreamEvent::LlmDelta {
        content: "ok".into(),
    };
    assert!(InteractiveController::should_render_terminal(true, &llm));
}

//! Mouse release handling: click matching and selection landing.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use tui_core::panel_select::TextPos;

use crate::interactive::TranscriptSelection;
use crate::modal::{ModalMouse, ModalMouseKind};
use crate::overlay::OverlayMode;
use crate::screens::ScreenKind;

use super::super::input_classify::{
    classify_mouse, click_kind_for_count, mouse_bypasses_capture, mouse_is_left_button,
    mouse_is_right_button, next_click_count, ClickKind, MouseAction, MouseOutcome,
};
use super::super::TuiApp;
use super::*;

impl TuiApp {
    /// Land one release against its press. Requires the same cell and the
    /// same snapshot generation; otherwise the pair is a drag. Right
    /// releases only copy the content line on a same-cell tap; badge,
    /// gutter, history and composer releases stay inert.
    pub(crate) fn handle_mouse_release(&mut self, mouse: MouseEvent) -> bool {
        let Some(press) = self.mouse_press.take() else {
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        };
        if let Some(session) = self.sessions.active_mut() {
            session.clear_transcript_autoscroll();
        }
        // Palette releases land only on the same overlay, cell and row
        // count; anything else is dropped without touching the session.
        if let Some(palette) = press.palette {
            if self.overlay != palette
                || press.col != mouse.column
                || press.row != mouse.row
                || !mouse_is_left_button(mouse.kind)
            {
                return false;
            }
            return self.land_palette_release(palette, press.palette_rows, mouse);
        }
        if press.is_right {
            if !mouse_is_right_button(mouse.kind) {
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                return false;
            }
            if self.mouse_layer_suppressed() {
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                return false;
            }
            let in_history = self.overlay == OverlayMode::History;
            if press.in_history != in_history || in_history {
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                return false;
            }
            if self.app.screen.navigation.current_kind() != ScreenKind::Interactive {
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                return false;
            }
            if press.in_gutter {
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                return false;
            }
            if press.in_composer {
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                return false;
            }
            let session_gen = self
                .sessions
                .active()
                .map(|session| session.hit_generation())
                .unwrap_or(0);
            if press.col != mouse.column || press.row != mouse.row {
                return false;
            }
            if press.session_gen != session_gen {
                return false;
            }
            let display_row = self.mouse_content_row(mouse);
            let Some(display_row) = display_row else {
                return false;
            };
            return self.copy_line_at(display_row);
        }
        if !mouse_is_left_button(mouse.kind) {
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        }
        if self.mouse_layer_suppressed() {
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        }
        let in_history = self.overlay == OverlayMode::History;
        if press.in_history != in_history {
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        }
        if in_history {
            // A drag selection only depends on logical positions, so a
            // redraw between press and release never invalidates it.
            if press.history_anchor {
                let text = self.history.finish_selection();
                if !self.copy_on_select_enabled() {
                    return text.is_some();
                }
                if let Some(text) = text {
                    let queued = self.queue_selection_copy(text);
                    if queued && self.clear_selection_after_copy() {
                        self.history.clear_selection();
                    }
                    return queued;
                }
                return false;
            }
            if press.history_gen != self.history_hit_generation {
                return false;
            }
            if press.col != mouse.column || press.row != mouse.row {
                return false;
            }
            let hit = self.history_badge.is_some_and(|rect| {
                crate::interactive::mouse_hit::badge_contains(&rect, mouse.column, mouse.row)
            });
            if hit {
                self.history.jump_to_latest();
                return true;
            }
            return false;
        }
        if self.app.screen.navigation.current_kind() != ScreenKind::Interactive {
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        }
        let session_gen = self
            .sessions
            .active()
            .map(|session| session.hit_generation())
            .unwrap_or(0);
        if press.in_composer {
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            if press.col != mouse.column || press.row != mouse.row {
                return false;
            }
            if press.session_gen != session_gen {
                return false;
            }
            return self.handle_composer_click(mouse);
        }
        if press.in_gutter {
            let dragging = self
                .sessions
                .active()
                .is_some_and(|session| session.is_gutter_dragging());
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            if press.gutter_thumb || dragging {
                return false;
            }
            if press.col != mouse.column || press.row != mouse.row {
                return false;
            }
            if press.session_gen != session_gen {
                return false;
            }
            if let Some(session) = self.sessions.active_mut() {
                return session.scroll_to_gutter_row(mouse.row);
            }
            return false;
        }
        if press.col != mouse.column || press.row != mouse.row {
            // A content drag release lands a transcript selection copy when
            // the pair shares the snapshot generation; cross-generation
            // pairs are stale drags and never copy.
            if let Some(anchor) = press.transcript_anchor {
                if press.session_gen != session_gen {
                    if let Some(session) = self.sessions.active_mut() {
                        session.clear_transcript_selection();
                    }
                    return false;
                }
                let head = self.content_anchor_for(mouse);
                let Some(head) = head else {
                    if let Some(session) = self.sessions.active_mut() {
                        session.clear_transcript_selection();
                    }
                    return false;
                };
                if anchor == head {
                    if let Some(session) = self.sessions.active_mut() {
                        session.clear_transcript_selection();
                    }
                    return false;
                }
                if !self.copy_on_select_enabled() {
                    // Keep the selection highlighted; no automatic copy.
                    return false;
                }
                return self.copy_transcript_selection(
                    TranscriptSelection { anchor, head },
                    self.clear_selection_after_copy(),
                );
            }
            return false;
        }
        if press.session_gen != session_gen {
            return false;
        }
        // An Alt press without a drag adds nothing; keep the committed
        // regions and run no click action on that cell.
        if press.transcript_add {
            return false;
        }
        if let Some(session) = self.sessions.active_mut() {
            session.clear_transcript_selection();
        }
        if !crate::interactive::mouse_hit::press_matches_release(
            press.col,
            press.row,
            press.session_gen,
            mouse.column,
            mouse.row,
            session_gen,
        ) {
            return false;
        }
        let hit = self.sessions.active().is_some_and(|session| {
            session.last_badge_rect().is_some_and(|rect| {
                crate::interactive::mouse_hit::badge_contains(&rect, mouse.column, mouse.row)
            })
        });
        if hit {
            if let Some(session) = self.sessions.active_mut() {
                session.jump_to_latest();
            }
            return true;
        }
        let display_row = self.mouse_content_row(mouse);
        let Some(display_row) = display_row else {
            return false;
        };
        let now_ms = crate::clock::now_ms();
        let count = next_click_count(
            self.last_click_count,
            self.last_click_ms,
            self.last_click_col,
            self.last_click_row,
            now_ms,
            mouse.column,
            mouse.row,
        );
        self.last_click_ms = Some(now_ms);
        self.last_click_col = Some(mouse.column);
        self.last_click_row = Some(mouse.row);
        self.last_click_count = count;
        match click_kind_for_count(count) {
            ClickKind::Double => {
                return self.copy_word_at(display_row, mouse.column);
            }
            ClickKind::Triple => {
                return self.copy_line_at(display_row);
            }
            ClickKind::Single => {}
        }
        // A prior selection wins over a single click: keep the range for
        // copying instead of toggling the row underneath.
        if press.had_selection {
            return false;
        }
        // Ctrl+Click on a sub-agent start row opens the child session
        // directly; every other row keeps its plain-click action.
        if mouse.modifiers.contains(KeyModifiers::CONTROL) {
            let child = self.sessions.active().and_then(|session| {
                match session.content_click_target(display_row) {
                    Some(crate::interactive::mouse_hit::ClickTarget::Subagent { seq }) => {
                        session.subagent_child_for_seq(seq)
                    }
                    _ => None,
                }
            });
            if let Some(id) = child {
                self.pending_mouse_action = Some(PendingMouseAction::OpenChildSession(id));
                return true;
            }
        }
        if let Some(session) = self.sessions.active_mut() {
            return session.handle_content_click(display_row);
        }
        false
    }
}

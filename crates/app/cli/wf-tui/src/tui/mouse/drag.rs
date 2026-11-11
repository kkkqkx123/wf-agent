//! Mouse drag and wheel handling: transcript anchors and scrolling.

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
    /// Anchor for transcript drag selection: logical text position for a
    /// press cell. `None` outside the content area.
    pub(crate) fn content_anchor_for(&self, mouse: MouseEvent) -> Option<TextPos> {
        let session = self.sessions.active()?;
        let area = session.last_scroll_area()?;
        let display_row = self.mouse_content_row(mouse)?;
        let display_col = mouse.column.saturating_sub(area.x) as usize;
        session.transcript_pos_at_display_row(display_row, display_col)
    }

    /// Translate a release cell into a scrollback display row. Returns
    /// `None` outside the viewport or inside the gutter column.
    pub(crate) fn mouse_content_row(&self, mouse: MouseEvent) -> Option<usize> {
        self.sessions.active().and_then(|session| {
            let area = session.last_scroll_area()?;
            if mouse.row < area.y || mouse.row >= area.y.saturating_add(area.height) {
                return None;
            }
            if let Some(gutter) = session.session_gutter_rect() {
                if mouse.column >= gutter.x {
                    return None;
                }
            } else if mouse.column < area.x || mouse.column >= area.x.saturating_add(area.width) {
                return None;
            }
            Some(mouse.row.saturating_sub(area.y) as usize)
        })
    }

    /// Consume one drag or move event. Only gutter thumb drags move the
    /// viewport; any drag breaks the multi-click chain and drops the press
    /// unless a thumb drag is in progress.
    pub(crate) fn handle_mouse_drag(&mut self, mouse: MouseEvent) -> bool {
        let Some(press) = self.mouse_press else {
            return false;
        };
        // Right drags stay native: drop the press without touching the
        // left multi-click chain.
        if press.is_right {
            self.mouse_press = None;
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        }
        // Any drag breaks the multi-click chain; scrolling drags
        // additionally move the viewport through the gutter path.
        self.clear_click_chain();
        if !mouse_is_left_button(mouse.kind) {
            self.mouse_press = None;
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        }
        if self.mouse_layer_suppressed() {
            self.mouse_press = None;
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        }
        let in_history = self.overlay == OverlayMode::History;
        if press.in_history != in_history {
            self.mouse_press = None;
            return false;
        }
        if in_history {
            if press.history_anchor {
                self.history.extend_selection(mouse.column, mouse.row);
                return true;
            }
            self.mouse_press = None;
            return false;
        }
        if press.in_composer {
            self.clear_click_chain();
            self.mouse_press = None;
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        }
        if press.in_gutter && press.gutter_thumb {
            if let Some(session) = self.sessions.active_mut() {
                return session.update_gutter_drag(mouse.row);
            }
            return false;
        }
        // Content drag with an anchor tracks a transcript selection for
        // highlight and release copy. The press stays armed so the release
        // can land the selection; any drag still breaks multi-click.
        if let Some(anchor) = press.transcript_anchor {
            if press.in_gutter || press.in_composer {
                self.mouse_press = None;
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                return false;
            }
            let Some(head) = self.content_anchor_for(mouse) else {
                // Drag head left the viewport vertically: arm auto-scroll,
                // tick one viewport notch and keep the press armed so the
                // release still lands the grown selection. Horizontal
                // overflow alone stays native and drops the press.
                let outside = self.sessions.active().and_then(|session| {
                    session.last_scroll_area().and_then(|area| {
                        crate::interactive::mouse_hit::autoscroll_dir_for_row(mouse.row, &area)
                    })
                });
                if let Some(direction) = outside {
                    if let Some(session) = self.sessions.active_mut() {
                        if !press.transcript_add {
                            session.clear_transcript_selection();
                        }
                        if session.transcript_selection().is_none() {
                            session.set_transcript_selection(TranscriptSelection {
                                anchor,
                                head: anchor,
                            });
                        }
                        session.start_transcript_autoscroll(direction);
                        session.tick_transcript_autoscroll();
                    }
                    return true;
                }
                self.mouse_press = None;
                if let Some(session) = self.sessions.active_mut() {
                    session.clear_transcript_selection();
                    session.end_gutter_drag();
                }
                return false;
            };
            if let Some(session) = self.sessions.active_mut() {
                if !press.transcript_add {
                    session.clear_transcript_selection();
                }
                session.set_transcript_selection(TranscriptSelection { anchor, head });
            }
            return true;
        }
        self.mouse_press = None;
        if let Some(session) = self.sessions.active_mut() {
            session.end_gutter_drag();
        }
        false
    }

    /// Apply one wheel notch to the content under the cursor.
    pub(crate) fn scroll_by_wheel(&mut self, up: bool) {
        if self.app.screen.navigation.current_kind() == ScreenKind::Interactive {
            if let Some(session) = self.sessions.active_mut() {
                if up {
                    session.scroll_history_up();
                } else {
                    session.scroll_history_down();
                }
                return;
            }
        }
        let len = self.nav_len();
        if up {
            self.app.screen.navigation.select_prev(len);
        } else {
            self.app.screen.navigation.select_next(len);
        }
    }
}

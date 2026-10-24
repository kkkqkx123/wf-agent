//! Mouse event handling for the TUI shell: wheel scrolling, click matching,
//! gutter drags, transcript selection and composer clicks.

use crossterm::event::{MouseEvent, MouseEventKind};

use crate::overlay::OverlayMode;
use crate::screens::ScreenKind;

use super::input_classify::{
    classify_mouse, click_kind_for_count, mouse_bypasses_capture, mouse_is_left_button,
    mouse_is_right_button, next_click_count, ClickKind, MouseAction, MouseOutcome,
};
use super::TuiApp;

/// Press record for click matching: the cell plus the snapshot generations
/// of both layers and which layer was on top. A release lands only on the
/// same cell of the same generation, otherwise it is a drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MousePress {
    col: u16,
    row: u16,
    session_gen: u64,
    history_gen: u64,
    in_history: bool,
    in_gutter: bool,
    gutter_thumb: bool,
    /// Composer prompt viewport press: releases place the cursor instead
    /// of touching scrollback hits.
    in_composer: bool,
    /// Content anchor for transcript drag selection: display row plus
    /// in-row display column at press time. `None` for gutter, badge,
    /// composer and history presses.
    transcript_anchor: Option<(usize, usize)>,
    /// Right-button press: only a same-cell release copies the line, any
    /// drag stays native and never lands.
    is_right: bool,
}

impl TuiApp {
    /// Handle a non-move mouse event. Motion is filtered by the caller.
    /// Wheel scrolls the scrollback on the session screen (reusing the
    /// pager-aware scroll methods) or moves the selection elsewhere.
    /// Shift plus any button keeps native selection and never requests a
    /// frame. A badge click (same cell and same snapshot generation inside
    /// the badge rectangle) jumps to the tail; anything else is ignored so
    /// native selection keeps working. Returns the outcome for the frame
    /// decision: scrolled or clicked requests a full frame, ignored
    /// requests nothing (no focus mark either).
    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent) -> MouseOutcome {
        if mouse_bypasses_capture(mouse.modifiers) {
            return MouseOutcome::Ignored;
        }
        match classify_mouse(mouse.kind) {
            MouseAction::ScrollUp => {
                self.mouse_press = None;
                self.clear_click_chain();
                self.clear_mouse_hover();
                if let Some(session) = self.sessions.active_mut() {
                    session.clear_transcript_selection();
                    session.end_gutter_drag();
                }
                self.scroll_by_wheel(true);
                MouseOutcome::Scrolled
            }
            MouseAction::ScrollDown => {
                self.mouse_press = None;
                self.clear_click_chain();
                self.clear_mouse_hover();
                if let Some(session) = self.sessions.active_mut() {
                    session.clear_transcript_selection();
                    session.end_gutter_drag();
                }
                self.scroll_by_wheel(false);
                MouseOutcome::Scrolled
            }
            MouseAction::IgnoredButton => {
                if self.handle_mouse_button(mouse) {
                    MouseOutcome::Clicked
                } else {
                    MouseOutcome::Ignored
                }
            }
        }
    }

    /// Whether all mouse hits are suppressed by a covering layer.
    fn mouse_layer_suppressed(&self) -> bool {
        if !self.modals.is_empty() {
            return true;
        }
        if self
            .sessions
            .active()
            .is_some_and(|session| session.approval_active())
        {
            return true;
        }
        matches!(
            self.overlay,
            OverlayMode::Sidebar | OverlayMode::SearchPalette | OverlayMode::SessionList
        )
    }

    /// Clear the multi-click chain after scroll, resize or drag.
    pub(super) fn clear_click_chain(&mut self) {
        self.last_click_ms = None;
        self.last_click_col = None;
        self.last_click_row = None;
        self.last_click_count = 0;
    }

    /// Record one hover cell from a motion event. Never requests a frame;
    /// the value is only consulted by the next full frame. Hover recording
    /// requires capture so disabled capture keeps the terminal untouched.
    pub(super) fn record_mouse_hover(&mut self, mouse: MouseEvent) {
        if !self.perf_policy.enable_mouse_capture {
            return;
        }
        if mouse_bypasses_capture(mouse.modifiers) {
            return;
        }
        if self.mouse_layer_suppressed()
            || self.app.screen.navigation.current_kind() != ScreenKind::Interactive
        {
            return;
        }
        self.mouse_hover = Some((mouse.column, mouse.row));
        if let Some(session) = self.sessions.active_mut() {
            session.set_hover_cell(mouse.column, mouse.row);
        }
    }

    /// Clear the hover cell after scroll, resize or button activity.
    pub(super) fn clear_mouse_hover(&mut self) {
        self.mouse_hover = None;
        if let Some(session) = self.sessions.active_mut() {
            session.clear_hover_cell();
        }
    }

    /// Land one composer click: single places the cursor, double copies the
    /// word, triple copies the buffer. Shares the content multi-click chain.
    fn handle_composer_click(&mut self, mouse: MouseEvent) -> bool {
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
            ClickKind::Double => self.copy_composer_word_at(mouse.column, mouse.row),
            ClickKind::Triple => self.copy_composer_text(),
            ClickKind::Single => match self.sessions.active_mut() {
                Some(session) => session.move_composer_cursor_to_cell(mouse.column, mouse.row),
                None => false,
            },
        }
    }

    /// Tab index under a terminal cell, if the cell lands on the tab
    /// strip. Cells divide evenly across open tabs.
    fn tab_index_at(&self, col: u16, row: u16) -> Option<usize> {
        let area = self.tab_strip_area?;
        if self.sessions.len() < 2 {
            return None;
        }
        if row != area.y || col < area.x || col >= area.x.saturating_add(area.width) {
            return None;
        }
        let width = usize::from(area.width.max(1));
        let len = self.sessions.len();
        Some(((usize::from(col - area.x) * len) / width).min(len - 1))
    }

    /// Route one button press, release or drag. Thin dispatcher over the
    /// three press / release / drag paths below. Column decides gutter
    /// first so the bar never competes with row hits.
    fn handle_mouse_button(&mut self, mouse: MouseEvent) -> bool {
        if !self.perf_policy.enable_mouse_capture {
            return false;
        }
        match mouse.kind {
            MouseEventKind::Down(_) => self.handle_mouse_press(mouse),
            MouseEventKind::Up(_) => self.handle_mouse_release(mouse),
            MouseEventKind::Drag(_) | MouseEventKind::Moved => self.handle_mouse_drag(mouse),
            _ => false,
        }
    }

    /// Record one press for click matching. Left presses participate in
    /// badge, gutter and content clicks; right presses only record for a
    /// same-cell line copy, any drag stays native.
    fn handle_mouse_press(&mut self, mouse: MouseEvent) -> bool {
        let is_right = mouse_is_right_button(mouse.kind);
        if !mouse_is_left_button(mouse.kind) && !is_right {
            return false;
        }
        if self.mouse_layer_suppressed() {
            return false;
        }
        if !is_right
            && self.overlay == OverlayMode::None
            && self.app.screen.navigation.current_kind() == ScreenKind::Interactive
        {
            if let Some(index) = self.tab_index_at(mouse.column, mouse.row) {
                if self.sessions.switch(index, self.now_ms()) {
                    self.save_tabs();
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                }
                return true;
            }
        }
        // A fresh press drops any previous drag selection.
        if let Some(session) = self.sessions.active_mut() {
            session.clear_transcript_selection();
        }
        let in_history = self.overlay == OverlayMode::History;
        if !in_history && self.app.screen.navigation.current_kind() != ScreenKind::Interactive {
            return false;
        }
        if !in_history {
            let inside_scroll = self.sessions.active().is_some_and(|session| {
                session.last_scroll_area().is_some_and(|area| {
                    mouse.column >= area.x
                        && mouse.column < area.x.saturating_add(area.width)
                        && mouse.row >= area.y
                        && mouse.row < area.y.saturating_add(area.height)
                })
            });
            if !inside_scroll {
                let inside_composer = self.sessions.active().is_some_and(|session| {
                    session.last_input_area().is_some_and(|area| {
                        crate::interactive::mouse_hit::composer_contains(
                            &area,
                            mouse.column,
                            mouse.row,
                        )
                    })
                });
                if !inside_composer {
                    return false;
                }
                let session_gen = self
                    .sessions
                    .active()
                    .map(|session| session.hit_generation())
                    .unwrap_or(0);
                self.mouse_press = Some(MousePress {
                    col: mouse.column,
                    row: mouse.row,
                    session_gen,
                    history_gen: self.history_hit_generation,
                    in_history,
                    in_gutter: false,
                    gutter_thumb: false,
                    in_composer: true,
                    transcript_anchor: None,
                    is_right,
                });
                self.clear_mouse_hover();
                return false;
            }
        }
        let session_gen = self
            .sessions
            .active()
            .map(|session| session.hit_generation())
            .unwrap_or(0);
        let (in_gutter, gutter_thumb) = if in_history {
            (false, false)
        } else {
            self.sessions.active().map_or((false, false), |session| {
                let gutter = session.session_gutter_rect();
                let hit = gutter.is_some_and(|rect| {
                    tui_components::scrollbar::gutter_contains(rect, mouse.column, mouse.row)
                });
                if hit {
                    let thumb = session.gutter_press_on_thumb(mouse.row);
                    (true, thumb)
                } else {
                    (false, false)
                }
            })
        };
        if in_gutter && gutter_thumb && !is_right {
            if let Some(session) = self.sessions.active_mut() {
                session.begin_gutter_drag();
            }
        }
        self.mouse_press = Some(MousePress {
            col: mouse.column,
            row: mouse.row,
            session_gen,
            history_gen: self.history_hit_generation,
            in_history,
            in_gutter,
            gutter_thumb,
            in_composer: false,
            transcript_anchor: if is_right || in_gutter {
                None
            } else {
                self.content_anchor_for(mouse)
            },
            is_right,
        });
        self.clear_mouse_hover();
        false
    }

    /// Land one release against its press. Requires the same cell and the
    /// same snapshot generation; otherwise the pair is a drag. Right
    /// releases only copy the content line on a same-cell tap; badge,
    /// gutter, history and composer releases stay inert.
    fn handle_mouse_release(&mut self, mouse: MouseEvent) -> bool {
        let Some(press) = self.mouse_press.take() else {
            if let Some(session) = self.sessions.active_mut() {
                session.end_gutter_drag();
            }
            return false;
        };
        if let Some(session) = self.sessions.active_mut() {
            session.clear_transcript_autoscroll();
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
            if let Some((anchor_row, anchor_col)) = press.transcript_anchor {
                if press.session_gen != session_gen {
                    if let Some(session) = self.sessions.active_mut() {
                        session.clear_transcript_selection();
                    }
                    return false;
                }
                let head = self.content_anchor_for(mouse);
                if let Some(session) = self.sessions.active_mut() {
                    session.clear_transcript_selection();
                }
                let Some((head_row, head_col)) = head else {
                    return false;
                };
                if anchor_row == head_row && anchor_col == head_col {
                    return false;
                }
                return self.copy_transcript_range(anchor_row, anchor_col, head_row, head_col);
            }
            return false;
        }
        if press.session_gen != session_gen {
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
        if let Some(session) = self.sessions.active_mut() {
            return session.handle_content_click(display_row);
        }
        false
    }

    /// Anchor for transcript drag selection: display row plus in-row
    /// display column for a press cell. `None` outside the content area.
    fn content_anchor_for(&self, mouse: MouseEvent) -> Option<(usize, usize)> {
        let display_row = self.mouse_content_row(mouse)?;
        let display_col = self
            .sessions
            .active()
            .and_then(|session| session.last_scroll_area())
            .map(|area| mouse.column.saturating_sub(area.x) as usize)?;
        Some((display_row, display_col))
    }

    /// Translate a release cell into a scrollback display row. Returns
    /// `None` outside the viewport or inside the gutter column.
    fn mouse_content_row(&self, mouse: MouseEvent) -> Option<usize> {
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
    fn handle_mouse_drag(&mut self, mouse: MouseEvent) -> bool {
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
        if press.in_history != in_history || in_history {
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
        if let Some((anchor_row, anchor_col)) = press.transcript_anchor {
            if press.in_gutter || press.in_composer {
                self.mouse_press = None;
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                return false;
            }
            let Some((head_row, head_col)) = self.content_anchor_for(mouse) else {
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
                        if session.transcript_selection().is_none() {
                            if let Some((anchor_row, anchor_col)) = press.transcript_anchor {
                                session.set_transcript_selection(
                                    crate::interactive::TranscriptSelection {
                                        anchor_row,
                                        anchor_col,
                                        head_row: anchor_row,
                                        head_col: anchor_col,
                                    },
                                );
                            }
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
                session.set_transcript_selection(crate::interactive::TranscriptSelection {
                    anchor_row,
                    anchor_col,
                    head_row,
                    head_col,
                });
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
    fn scroll_by_wheel(&mut self, up: bool) {
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

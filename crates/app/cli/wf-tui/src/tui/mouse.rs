//! Mouse event handling for the TUI shell: wheel scrolling, click matching,
//! gutter drags, transcript selection and composer clicks.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use tui_core::panel_select::TextPos;

use crate::interactive::TranscriptSelection;
use crate::modal::{ModalMouse, ModalMouseKind};
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
    /// Content anchor for transcript drag selection: logical text position at
    /// press time. `None` for gutter, badge, composer and history presses.
    transcript_anchor: Option<TextPos>,
    /// Press landed inside the history pager text area and armed a drag
    /// selection there.
    history_anchor: bool,
    /// Alt press: keep the existing transcript regions and add another one
    /// instead of replacing the selection.
    transcript_add: bool,
    /// Press landed while a transcript selection existed. The press keeps
    /// it: a same-cell release runs no click action so copy gestures are
    /// never eaten, while a drag replaces it with the new range.
    had_selection: bool,
    /// Right-button press: only a same-cell release copies the line, any
    /// drag stays native and never lands.
    is_right: bool,
    /// Palette overlay press: which palette was on top. A release lands
    /// only on the same overlay with the same displayed row count; a
    /// refilter between press and release invalidates the pair.
    palette: Option<OverlayMode>,
    /// Displayed row count of the palette layout at press time.
    palette_rows: usize,
}

/// Map a terminal mouse event to the backend-free gesture a modal handles.
/// Only the left button takes part: right and middle presses stay inert so
/// native terminal menus keep working. Alt plus a press asks the modal to
/// keep its existing region and start another one.
fn modal_mouse_kind(mouse: MouseEvent) -> Option<ModalMouseKind> {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) if mouse.modifiers.contains(KeyModifiers::ALT) => {
            Some(ModalMouseKind::PressAdd)
        }
        MouseEventKind::Down(MouseButton::Left) => Some(ModalMouseKind::Press),
        MouseEventKind::Drag(MouseButton::Left) => Some(ModalMouseKind::Drag),
        MouseEventKind::Up(MouseButton::Left) => Some(ModalMouseKind::Release),
        MouseEventKind::ScrollUp => Some(ModalMouseKind::ScrollUp),
        MouseEventKind::ScrollDown => Some(ModalMouseKind::ScrollDown),
        _ => None,
    }
}

/// Async mouse action armed by a sync release path and drained once per
/// event-loop turn before drawing. Click routing itself stays frame-pure;
/// only the armed action awaits.
#[derive(Debug)]
pub(super) enum PendingMouseAction {
    /// Run the selected command palette row with the typed argument.
    RunPaletteCommand,
    /// Open a new chat tab from the strip new-tab cell.
    NewTab,
    /// Close the tab strip tab at the index.
    CloseTab(usize),
    /// Open the child session behind a sub-agent start row.
    OpenChildSession(String),
}

impl TuiApp {
    /// Handle a non-move mouse event. Motion is filtered by the caller.
    /// Wheel scrolls the scrollback on the session screen (reusing the
    /// pager-aware scroll methods) or moves the selection elsewhere.
    /// Shift plus any button keeps native selection and never requests a
    /// frame. Buttons route to press, release and drag handling: the tab
    /// strip switches, closes or opens tabs, palette rows focus or run,
    /// the badge jumps to the tail, the gutter jumps or drags the
    /// viewport, content rows toggle tool blocks, jump to sub agents
    /// (Ctrl opens them) or grow a transcript selection, and the composer
    /// places the cursor. A prior transcript selection wins over a single
    /// content click. Releases outside any hit stay silent so native
    /// selection keeps working. Returns the outcome for the frame
    /// decision: scrolled or clicked requests a full frame, ignored
    /// requests nothing (no focus mark either).
    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent) -> MouseOutcome {
        if mouse_bypasses_capture(mouse.modifiers) {
            return MouseOutcome::Ignored;
        }
        // An open modal owns the pointer: wheel and button events go to the
        // top modal instead of the screen below it.
        if !self.modals.is_empty() {
            return self.handle_modal_mouse(mouse);
        }
        match classify_mouse(mouse.kind) {
            MouseAction::ScrollUp => {
                self.mouse_press = None;
                self.clear_click_chain();
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                self.scroll_by_wheel(true);
                MouseOutcome::Scrolled
            }
            MouseAction::ScrollDown => {
                self.mouse_press = None;
                self.clear_click_chain();
                if let Some(session) = self.sessions.active_mut() {
                    session.end_gutter_drag();
                }
                self.scroll_by_wheel(false);
                MouseOutcome::Scrolled
            }
            MouseAction::Button => {
                if self.handle_mouse_button(mouse) {
                    MouseOutcome::Clicked
                } else {
                    MouseOutcome::Ignored
                }
            }
        }
    }

    /// Route one gesture to the top modal. A modal that reports changed
    /// content or produced selection text requests a frame; text goes through
    /// the same sanitized clipboard queue as the keyboard copy paths.
    fn handle_modal_mouse(&mut self, mouse: MouseEvent) -> MouseOutcome {
        if !self.perf_policy.enable_mouse_capture {
            return MouseOutcome::Ignored;
        }
        let Some(kind) = modal_mouse_kind(mouse) else {
            return MouseOutcome::Ignored;
        };
        match self.modals.handle_mouse(kind, mouse.column, mouse.row) {
            ModalMouse::Ignored => MouseOutcome::Ignored,
            ModalMouse::Changed => MouseOutcome::Clicked,
            ModalMouse::Copy(text) => {
                if !self.copy_on_select_enabled() {
                    return MouseOutcome::Clicked;
                }
                self.queue_selection_copy(text);
                if self.clear_selection_after_copy() {
                    self.modals.clear_selection();
                }
                MouseOutcome::Clicked
            }
        }
    }

    /// Whether all mouse hits are suppressed by a covering layer: modals,
    /// blocking footer views (approval or question) and full-screen
    /// overlays. The history overlay is not listed: it has its own badge
    /// and selection routing on a separate snapshot.
    pub(super) fn mouse_layer_suppressed(&self) -> bool {
        if !self.modals.is_empty() {
            return true;
        }
        if self
            .sessions
            .active()
            .is_some_and(|session| session.approval_active() || session.footer.question.is_some())
        {
            return true;
        }
        matches!(
            self.overlay,
            OverlayMode::Sidebar
                | OverlayMode::SearchPalette
                | OverlayMode::CommandPalette
                | OverlayMode::SessionList
                | OverlayMode::SessionTree
        )
    }

    /// Clear the multi-click chain after scroll, resize or drag.
    pub(super) fn clear_click_chain(&mut self) {
        self.last_click_ms = None;
        self.last_click_col = None;
        self.last_click_row = None;
        self.last_click_count = 0;
    }

    /// Record one hover cell from a motion event. Returns whether the
    /// affordance flipped and a full frame is due; repeats and moves that
    /// keep the painted underline unchanged stay frameless. Hover recording
    /// requires capture so disabled capture keeps the terminal untouched.
    pub(super) fn record_mouse_hover(&mut self, mouse: MouseEvent) -> bool {
        if !self.perf_policy.enable_mouse_capture {
            return false;
        }
        if mouse_bypasses_capture(mouse.modifiers) {
            return false;
        }
        if self.mouse_layer_suppressed()
            || self.app.screen.navigation.current_kind() != ScreenKind::Interactive
        {
            return false;
        }
        let Some(session) = self.sessions.active_mut() else {
            return false;
        };
        use crate::interactive::mouse_hit::{hover_frame_needed, HoverTarget};
        let old_cell = session.hover_cell();
        let old_target = old_cell.map_or(HoverTarget::None, |(col, row)| {
            session.hover_target_at(col, row)
        });
        let new_target = session.hover_target_at(mouse.column, mouse.row);
        let needs_frame =
            hover_frame_needed(old_cell, &old_target, mouse.column, mouse.row, &new_target);
        session.set_hover_cell(mouse.column, mouse.row);
        needs_frame
    }

    /// Clear the hover cell after resize or button activity.
    pub(super) fn clear_mouse_hover(&mut self) {
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
    /// Tab strip hit under a terminal cell, using the shared strip layout
    /// so clicks land where the cells are drawn. `None` without a
    /// published strip or outside every actionable cell.
    fn tab_strip_hit_at(&self, col: u16, row: u16) -> Option<tui_components::tab_bar::TabStripHit> {
        use crate::tab_bar::{layout_tab_strip, tab_strip_hit, SessionTabItem};
        let area = self.tab_strip_area?;
        let items: Vec<SessionTabItem> = self
            .sessions
            .snapshot()
            .into_iter()
            .map(|(title, streaming, pinned)| {
                let mut item = SessionTabItem::new(title, streaming);
                item.pinned = pinned;
                item
            })
            .collect();
        let layout = layout_tab_strip(area, &items, self.sessions.active_index());
        tab_strip_hit(&layout, area.y, col, row)
    }

    /// Land one tab strip hit: switch tabs, arm a tab close or arm a new
    /// tab. Async work arms a deferred action so release paths stay sync.
    fn land_tab_strip_hit(&mut self, hit: tui_components::tab_bar::TabStripHit) -> bool {
        use tui_components::tab_bar::TabStripHit;
        match hit {
            TabStripHit::Tab(index) => {
                if self.sessions.switch(index, self.now_ms()) {
                    self.request_save_tabs();
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                }
                true
            }
            TabStripHit::NewTab => {
                self.pending_mouse_action = Some(PendingMouseAction::NewTab);
                true
            }
            TabStripHit::CloseTab(index) => {
                self.pending_mouse_action = Some(PendingMouseAction::CloseTab(index));
                true
            }
        }
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
            if let Some(hit) = self.tab_strip_hit_at(mouse.column, mouse.row) {
                return self.land_tab_strip_hit(hit);
            }
        }
        // Palette overlays own their rows: a left press inside the list
        // arms a same-cell release that focuses (search) or runs
        // (command) the row. Clicks outside the list stay suppressed.
        if !is_right
            && matches!(
                self.overlay,
                OverlayMode::CommandPalette | OverlayMode::SearchPalette
            )
        {
            return self.handle_palette_press(mouse);
        }
        // A press never drops a previous drag selection; the release
        // decides: a same-cell release with a prior selection keeps it
        // and runs no click, a drag replaces it below. Alt+Click keeps
        // the committed regions so several disjoint ranges can be copied.
        let transcript_add = !is_right && mouse.modifiers.contains(KeyModifiers::ALT);
        let had_selection = !transcript_add
            && self
                .sessions
                .active()
                .is_some_and(|session| session.has_transcript_selection());
        if transcript_add {
            if let Some(session) = self.sessions.active_mut() {
                session.begin_additive_transcript_selection();
            }
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
                    history_anchor: false,
                    transcript_add: false,
                    had_selection: false,
                    is_right,
                    palette: None,
                    palette_rows: 0,
                });
                self.clear_mouse_hover();
                return false;
            }
        }
        // Palette overlays own their rows (see the early press routing
        // above); the scrollback paths below never run for them.
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
        // A fresh left press in the history pager arms a drag selection; any
        // previous region is dropped so each press starts a new range.
        let history_anchor = if in_history && !is_right {
            self.history.clear_selection();
            self.history.begin_selection(mouse.column, mouse.row)
        } else {
            false
        };
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
            history_anchor,
            transcript_add,
            had_selection,
            is_right,
            palette: None,
            palette_rows: 0,
        });
        self.clear_mouse_hover();
        false
    }

    /// Arm one palette row press. Only left presses inside the published
    /// list area arm; clicks outside the list stay suppressed without
    /// arming so the overlay never closes by accident.
    fn handle_palette_press(&mut self, mouse: MouseEvent) -> bool {
        let overlay = self.overlay;
        let layout = match overlay {
            OverlayMode::CommandPalette => self.command_palette_hit.clone(),
            OverlayMode::SearchPalette => self.search_palette_hit.clone(),
            _ => None,
        };
        let Some(layout) = layout else {
            return false;
        };
        if layout.item_at(mouse.column, mouse.row).is_none() {
            return false;
        }
        self.mouse_press = Some(MousePress {
            col: mouse.column,
            row: mouse.row,
            session_gen: 0,
            history_gen: 0,
            in_history: false,
            in_gutter: false,
            gutter_thumb: false,
            in_composer: false,
            transcript_anchor: None,
            history_anchor: false,
            transcript_add: false,
            had_selection: false,
            is_right: false,
            palette: Some(overlay),
            palette_rows: layout.targets.len(),
        });
        self.clear_mouse_hover();
        false
    }

    /// Land one palette release: same overlay, same cell, same displayed
    /// row count. Command rows focus and arm an async run; search rows
    /// only focus, matching the keyboard (results have no open action).
    fn land_palette_release(
        &mut self,
        palette: OverlayMode,
        palette_rows: usize,
        mouse: MouseEvent,
    ) -> bool {
        let layout = match palette {
            OverlayMode::CommandPalette => self.command_palette_hit.clone(),
            OverlayMode::SearchPalette => self.search_palette_hit.clone(),
            _ => None,
        };
        let Some(layout) = layout else {
            return false;
        };
        if layout.targets.len() != palette_rows {
            return false;
        }
        let Some(index) = layout.item_at(mouse.column, mouse.row) else {
            return false;
        };
        match palette {
            OverlayMode::CommandPalette => {
                self.command_palette.select(index);
                self.pending_mouse_action = Some(PendingMouseAction::RunPaletteCommand);
                true
            }
            OverlayMode::SearchPalette => {
                self.search_palette.select(index);
                true
            }
            _ => false,
        }
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

    /// Anchor for transcript drag selection: logical text position for a
    /// press cell. `None` outside the content area.
    fn content_anchor_for(&self, mouse: MouseEvent) -> Option<TextPos> {
        let session = self.sessions.active()?;
        let area = session.last_scroll_area()?;
        let display_row = self.mouse_content_row(mouse)?;
        let display_col = mouse.column.saturating_sub(area.x) as usize;
        session.transcript_pos_at_display_row(display_row, display_col)
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

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
    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> MouseOutcome {
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
    pub(crate) fn handle_modal_mouse(&mut self, mouse: MouseEvent) -> MouseOutcome {
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
    pub(crate) fn mouse_layer_suppressed(&self) -> bool {
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
    pub(crate) fn clear_click_chain(&mut self) {
        self.last_click_ms = None;
        self.last_click_col = None;
        self.last_click_row = None;
        self.last_click_count = 0;
    }

    /// Record one hover cell from a motion event. Returns whether the
    /// affordance flipped and a full frame is due; repeats and moves that
    /// keep the painted underline unchanged stay frameless. Hover recording
    /// requires capture so disabled capture keeps the terminal untouched.
    pub(crate) fn record_mouse_hover(&mut self, mouse: MouseEvent) -> bool {
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
    pub(crate) fn clear_mouse_hover(&mut self) {
        if let Some(session) = self.sessions.active_mut() {
            session.clear_hover_cell();
        }
    }

    /// Land one composer click: single places the cursor, double copies the
    /// word, triple copies the buffer. Shares the content multi-click chain.
    pub(crate) fn handle_composer_click(&mut self, mouse: MouseEvent) -> bool {
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
    pub(crate) fn tab_strip_hit_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<tui_components::tab_bar::TabStripHit> {
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
    pub(crate) fn land_tab_strip_hit(&mut self, hit: tui_components::tab_bar::TabStripHit) -> bool {
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
    pub(crate) fn handle_mouse_button(&mut self, mouse: MouseEvent) -> bool {
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
}

mod drag;
mod press;
mod release;

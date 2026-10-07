//! Key routing and navigation for the TUI shell: global key handling,
//! overlay-specific keys, search input and screen navigation.

use crate::error::CliResult;
use crate::interactive::InteractiveAction;
use crate::keymap::{CKey, Key};
use crate::modal::HelpModal;
use crate::overlay::{LoopAction, OverlayMode};
use crate::screens::{ScreenData, ScreenKind};

use super::input_classify::{digit_to_screen, next_filter};
use super::signals::SUSPEND_PENDING;
use super::{TuiApp, DASHBOARD_ENTRIES};

impl TuiApp {
    pub(super) fn handle_key(&mut self, key: Key) -> CliResult<LoopAction> {
        // Ctrl-Z suspends the TUI like a terminal job. Raw mode disables
        // ISIG, so the keystroke never reaches the kernel as SIGTSTP; flag
        // the request here and let the loop run the restore / raise cycle.
        if key.ctrl && key.code == CKey::Char('z') {
            SUSPEND_PENDING.store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(LoopAction::Continue);
        }

        // Ctrl-C: quit if not in interactive, otherwise handled by interactive controller
        if key.ctrl
            && key.code == CKey::Char('c')
            && self.app.screen.navigation.current_kind() != ScreenKind::Interactive
        {
            return Ok(LoopAction::Quit);
        }

        // An open modal swallows every key.
        if !self.modals.is_empty() {
            let _ = self.modals.handle_key(key);
            return Ok(LoopAction::Continue);
        }

        // Handle overlay-specific keys
        if self.overlay != OverlayMode::None {
            return self.handle_overlay_key(key);
        }

        // Global shortcuts (work in any mode)
        match key.code {
            // Ctrl+T: open the history overlay on the session under focus
            CKey::Char('t') if key.ctrl => {
                let Some(session_id) = self.history_session_id() else {
                    self.app.notice.set("No session selected for history");
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                };
                self.history.begin(session_id);
                self.request_history_page(None);
                self.overlay = OverlayMode::History;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Ctrl+B: Toggle sidebar overlay
            CKey::Char('b') if key.ctrl => {
                self.overlay = OverlayMode::Sidebar;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // /: Open command palette (when in interactive)
            CKey::Char('/')
                if self.app.screen.navigation.current_kind() == ScreenKind::Interactive
                    && !key.ctrl
                    && !key.alt =>
            {
                // For now, show sidebar as a simple command palette
                self.overlay = OverlayMode::Sidebar;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            _ => {}
        }

        // The interactive screen owns all keys while active.
        if self.app.screen.navigation.current_kind() == ScreenKind::Interactive {
            if let Some(session) = &mut self.interactive {
                match session.handle_key(key) {
                    InteractiveAction::Continue => return Ok(LoopAction::Continue),
                    InteractiveAction::Exit => {
                        self.interactive_exit = true;
                        return Ok(LoopAction::Continue);
                    }
                }
            }
        }

        // The search screen owns printable input.
        if self.app.screen.navigation.current_kind() == ScreenKind::Search {
            return Ok(self.handle_search_key(key));
        }

        match key.code {
            CKey::Char('?') => {
                self.modals.push(Box::new(HelpModal));
            }
            CKey::Char('q') | CKey::Esc => {
                if !self.go_back() {
                    return Ok(LoopAction::Quit);
                }
            }
            CKey::Char(c) if c.is_ascii_digit() => {
                if let Some(kind) = digit_to_screen(c) {
                    self.goto(kind);
                }
            }
            CKey::Char('j') | CKey::Down => {
                let len = self.nav_len();
                self.app.screen.navigation.select_next(len);
            }
            CKey::Char('k') | CKey::Up => {
                let len = self.nav_len();
                self.app.screen.navigation.select_prev(len);
            }
            CKey::Enter => match self.app.screen.navigation.current_kind() {
                ScreenKind::Dashboard => {
                    let idx = self.app.screen.navigation.selected();
                    if let Some(kind) = DASHBOARD_ENTRIES.get(idx).copied() {
                        self.goto(kind);
                    }
                }
                ScreenKind::Executions | ScreenKind::AgentLoops => {
                    // Both list screens share the same shape: pick the selected
                    // id and open it as an interactive session replay.
                    let rows_len = self.current_data().row_count();
                    let selected = self
                        .app
                        .screen
                        .navigation
                        .selected()
                        .min(rows_len.saturating_sub(1));
                    let id = match self.current_data() {
                        ScreenData::Executions(rows) => rows.get(selected).map(|r| r.id.clone()),
                        ScreenData::AgentLoops(rows) => rows.get(selected).map(|r| r.id.clone()),
                        _ => None,
                    };
                    if let Some(id) = id {
                        self.goto(ScreenKind::Interactive);
                        self.pending_replay = Some(id);
                    }
                }
                _ => {}
            },
            CKey::Char('f')
                if self.app.screen.navigation.current_kind() == ScreenKind::Executions =>
            {
                self.app.screen.exec_filter = next_filter(self.app.screen.exec_filter);
                self.invalidate(ScreenKind::Executions);
                self.app
                    .notice
                    .set(format!("Filter: {}", self.app.screen.exec_filter.label()));
            }
            CKey::Char('r') => {
                // Manual refresh of the current screen.
                let kind = self.app.screen.navigation.current_kind();
                self.invalidate(kind);
                self.app
                    .notice
                    .set(format!("Refreshing {}...", kind.title()));
            }
            CKey::Char('p')
                if self.app.screen.navigation.current_kind() == ScreenKind::Executions =>
            {
                self.toggle_selected_execution();
            }
            CKey::Char('x')
                if self.app.screen.navigation.current_kind() == ScreenKind::Executions =>
            {
                self.cancel_selected_execution();
            }
            CKey::Char('d')
                if self.app.screen.navigation.current_kind() == ScreenKind::Workflow =>
            {
                self.delete_selected_workflow();
            }
            CKey::Char('m')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.pick_default_model();
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }

    pub(super) fn handle_overlay_key(&mut self, key: Key) -> CliResult<LoopAction> {
        match key.code {
            // Escape or q: close overlay
            CKey::Esc | CKey::Char('q') if !key.ctrl => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Ctrl+T: close history overlay
            CKey::Char('t') if key.ctrl && self.overlay == OverlayMode::History => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Ctrl+B: close sidebar overlay
            CKey::Char('b') if key.ctrl && self.overlay == OverlayMode::Sidebar => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Number keys: navigate to screen (sidebar mode)
            CKey::Char(c) if c.is_ascii_digit() && self.overlay == OverlayMode::Sidebar => {
                if let Some(kind) = digit_to_screen(c) {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
            }
            // j/k or arrows: navigate sidebar
            CKey::Char('j') | CKey::Down if self.overlay == OverlayMode::Sidebar => {
                let len = DASHBOARD_ENTRIES.len();
                self.app.screen.navigation.select_next(len);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            CKey::Char('k') | CKey::Up if self.overlay == OverlayMode::Sidebar => {
                let len = DASHBOARD_ENTRIES.len();
                self.app.screen.navigation.select_prev(len);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Enter: select from sidebar
            CKey::Enter if self.overlay == OverlayMode::Sidebar => {
                let idx = self.app.screen.navigation.selected();
                if let Some(kind) = DASHBOARD_ENTRIES.get(idx).copied() {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
            }
            // History overlay: a PageUp resting on the oldest loaded row asks
            // for the page behind it; anywhere else it moves the viewport.
            CKey::PageUp if self.overlay == OverlayMode::History => {
                if let Some(before) = self.history.start_earlier() {
                    self.request_history_page(Some(before));
                } else {
                    self.history.scroll_up();
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // History overlay: walk back down towards the tail.
            CKey::PageDown if self.overlay == OverlayMode::History => {
                self.history.scroll_down();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }

    pub(super) fn handle_search_key(&mut self, key: Key) -> LoopAction {
        match key.code {
            CKey::Esc | CKey::Char('q') if !key.ctrl => {
                // `q` only leaves the screen when the draft is empty.
                if key.code == CKey::Esc || self.app.screen.search_input.is_empty() {
                    if !self.go_back() {
                        return LoopAction::Quit;
                    }
                } else {
                    self.app.screen.search_input.push('q');
                }
            }
            CKey::Enter => {
                let query = self.app.screen.search_input.trim().to_string();
                if query.is_empty() {
                    self.app.notice.set("Enter a query to search.");
                } else {
                    self.invalidate(ScreenKind::Search);
                    self.app.notice.set(format!("Searching for \"{query}\"..."));
                }
            }
            CKey::Backspace => {
                self.app.screen.search_input.pop();
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.app.screen.search_input.push(c);
            }
            CKey::Char('?') => {
                self.modals.push(Box::new(HelpModal));
            }
            CKey::Char(c) if c.is_ascii_digit() && key.alt => {
                if let Some(kind) = digit_to_screen(c) {
                    self.goto(kind);
                }
            }
            _ => {}
        }
        LoopAction::Continue
    }

    /// Number of selectable rows on the current screen.
    pub(super) fn nav_len(&self) -> usize {
        match self.app.screen.navigation.current_kind() {
            ScreenKind::Dashboard => DASHBOARD_ENTRIES.len(),
            ScreenKind::Help => 1,
            kind if kind.has_data() => self.current_data().row_count().max(1),
            _ => 1,
        }
    }

    pub(super) fn goto(&mut self, kind: ScreenKind) {
        self.app.screen.navigation.navigate_to(kind);
        self.request_data(kind);
    }

    pub(super) fn go_back(&mut self) -> bool {
        if self.app.screen.navigation.go_back() {
            let kind = self.app.screen.navigation.current_kind();
            self.request_data(kind);
            true
        } else {
            false
        }
    }
}

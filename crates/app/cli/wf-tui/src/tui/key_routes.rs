//! Key routing and navigation for the TUI shell: global key handling,
//! overlay-specific keys and screen navigation.

use crate::error::CliResult;
use crate::interactive::InteractiveAction;
use crate::keymap::{CKey, Key};
use crate::modal::HelpModal;
use crate::overlay::{LoopAction, OverlayMode};
use crate::screens::{ScreenData, ScreenKind};

use super::input_classify::next_filter;
use super::signals::SUSPEND_PENDING;
use super::{TuiApp, DASHBOARD_ENTRIES};
use tui_core::screen_data::screen_for_digit;

impl TuiApp {
    pub(super) fn handle_key(&mut self, key: Key) -> CliResult<LoopAction> {
        if key.ctrl && key.code == CKey::Char('z') {
            SUSPEND_PENDING.store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(LoopAction::Continue);
        }

        if key.ctrl
            && key.code == CKey::Char('c')
            && self.app.screen.navigation.current_kind() != ScreenKind::Interactive
        {
            return Ok(LoopAction::Quit);
        }

        if !self.modals.is_empty() {
            if key.ctrl && !key.alt && matches!(key.code, CKey::Char('y') | CKey::Char('Y')) {
                if let Some(text) = self.modals.copy_source() {
                    if self.copy_policy.allowed {
                        match crate::terminal::sanitize_copy_text(&text) {
                            Some(clean) => {
                                self.pending_copy =
                                    Some(crate::copy::PendingCopy::new(clean, "viewer"));
                            }
                            None => self.app.notice.set("copy: viewer is empty".to_string()),
                        }
                    } else {
                        self.app
                            .notice
                            .set(self.copy_policy.deny_notice().to_string());
                    }
                    return Ok(LoopAction::Continue);
                }
            }
            let _ = self.modals.handle_key(key);
            return Ok(LoopAction::Continue);
        }

        if self.overlay != OverlayMode::None {
            return self.handle_overlay_key(key);
        }

        match key.code {
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
            CKey::Char('b') if key.ctrl => {
                self.overlay = OverlayMode::Sidebar;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            CKey::Char('k') if key.ctrl && !key.alt => {
                self.overlay = OverlayMode::SearchPalette;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            _ => {}
        }

        if self.app.screen.navigation.current_kind() == ScreenKind::Interactive {
            if let Some(session) = &mut self.interactive {
                match session.handle_key(key) {
                    InteractiveAction::Continue => {}
                    InteractiveAction::Exit => {
                        self.interactive_exit = true;
                        return Ok(LoopAction::Continue);
                    }
                }
                if let Some(request) = session.take_tool_viewer() {
                    if request.is_diff {
                        self.modals
                            .push(Box::new(crate::file_viewer::DiffViewer::from_unified(
                                request.title,
                                &request.content,
                            )));
                    } else {
                        self.modals
                            .push(Box::new(crate::file_viewer::FileViewer::new(
                                request.title,
                                &request.content,
                            )));
                    }
                }
                if let Some(copy) = session.take_pending_copy() {
                    self.pending_copy = Some(copy);
                }
                if let Some(notice) = session.take_copy_notice() {
                    self.app.notice.set(notice);
                }
                return Ok(LoopAction::Continue);
            }
        }

        if self.app.screen.navigation.current_kind() == ScreenKind::Settings {
            match key.code {
                CKey::Char('j') | CKey::Down if !key.ctrl && !key.alt => {
                    self.scroll_settings(1);
                    return Ok(LoopAction::Continue);
                }
                CKey::Char('k') | CKey::Up if !key.ctrl && !key.alt => {
                    self.scroll_settings(-1);
                    return Ok(LoopAction::Continue);
                }
                CKey::PageDown if !key.ctrl && !key.alt => {
                    self.scroll_settings(10);
                    return Ok(LoopAction::Continue);
                }
                CKey::PageUp if !key.ctrl && !key.alt => {
                    self.scroll_settings(-10);
                    return Ok(LoopAction::Continue);
                }
                CKey::Home if !key.ctrl && !key.alt => {
                    self.app.screen.settings_scroll = 0;
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
                CKey::End if !key.ctrl && !key.alt => {
                    self.app.screen.settings_scroll = usize::MAX / 2;
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
                _ => {}
            }
        }

        match key.code {
            CKey::Char('?') if !key.ctrl && !key.alt => {
                self.modals.push(Box::new(HelpModal));
            }
            CKey::Char('q') | CKey::Esc => {
                if !self.go_back() {
                    return Ok(LoopAction::Quit);
                }
            }
            CKey::Char(c) if c.is_ascii_digit() && !key.ctrl && !key.alt => {
                if let Some(kind) = screen_for_digit(c) {
                    self.goto(kind);
                } else if c == '7' {
                    self.overlay = OverlayMode::SearchPalette;
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
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
                ScreenKind::Executions => {
                    let rows_len = self.current_data().row_count();
                    let selected = self
                        .app
                        .screen
                        .navigation
                        .selected()
                        .min(rows_len.saturating_sub(1));
                    let id = match self.current_data() {
                        ScreenData::Executions(rows) => rows.get(selected).map(|r| r.id.clone()),
                        _ => None,
                    };
                    if let Some(id) = id {
                        self.goto(ScreenKind::Interactive);
                        self.pending_replay = Some(id);
                    }
                }
                ScreenKind::Checkpoints => {
                    self.open_selected_checkpoint_detail();
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
                let kind = self.app.screen.navigation.current_kind();
                self.invalidate(kind);
                self.app
                    .notice
                    .set(format!("Refreshing {}...", kind.title()));
            }
            CKey::Char('R')
                if self.app.screen.navigation.current_kind() == ScreenKind::Checkpoints =>
            {
                self.restore_selected_checkpoint();
            }
            CKey::Char('t')
                if self.app.screen.navigation.current_kind() == ScreenKind::Checkpoints =>
            {
                self.open_selected_checkpoint_chain();
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
            CKey::Char('d')
                if self.app.screen.navigation.current_kind() == ScreenKind::Checkpoints =>
            {
                self.delete_selected_checkpoint();
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
        if self.overlay == OverlayMode::SearchPalette {
            return Ok(self.handle_search_palette_key(key));
        }
        match key.code {
            CKey::Esc | CKey::Char('q') if !key.ctrl => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            CKey::Char('t') if key.ctrl && self.overlay == OverlayMode::History => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            CKey::Char('b') if key.ctrl && self.overlay == OverlayMode::Sidebar => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            CKey::Char(c) if c.is_ascii_digit() && self.overlay == OverlayMode::Sidebar => {
                if let Some(kind) = screen_for_digit(c) {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
            }
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
            CKey::Enter if self.overlay == OverlayMode::Sidebar => {
                let idx = self.app.screen.navigation.selected();
                if let Some(kind) = DASHBOARD_ENTRIES.get(idx).copied() {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
            }
            CKey::PageUp if self.overlay == OverlayMode::History => {
                if let Some(before) = self.history.start_earlier() {
                    self.request_history_page(Some(before));
                } else {
                    self.history.scroll_up();
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            CKey::PageDown if self.overlay == OverlayMode::History => {
                self.history.scroll_down();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }

    fn handle_search_palette_key(&mut self, key: Key) -> LoopAction {
        match key.code {
            CKey::Esc => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('k') if key.ctrl && !key.alt => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Enter => {
                match self.search_palette.submit() {
                    Some(query) => {
                        self.request_search_palette(query.clone());
                        self.app.notice.set(format!("Searching for \"{query}\"..."));
                    }
                    None => self.app.notice.set("Enter a query to search."),
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Backspace if !key.ctrl && !key.alt => {
                self.search_palette.pop();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('j') if key.ctrl || key.alt => {
                self.search_palette.move_next();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('k') if key.ctrl || key.alt => {
                self.search_palette.move_prev();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Down => {
                self.search_palette.move_next();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Up => {
                self.search_palette.move_prev();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::PageDown => {
                self.search_palette.page_down();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::PageUp => {
                self.search_palette.page_up();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.search_palette.push_char(c);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char(c) if c.is_ascii_digit() && key.alt => {
                if let Some(kind) = screen_for_digit(c) {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                }
            }
            _ => {}
        }
        LoopAction::Continue
    }

    fn scroll_settings(&mut self, delta: isize) {
        if delta >= 0 {
            self.app.screen.settings_scroll = self
                .app
                .screen
                .settings_scroll
                .saturating_add(delta as usize);
        } else {
            self.app.screen.settings_scroll = self
                .app
                .screen
                .settings_scroll
                .saturating_sub(delta.unsigned_abs());
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Number of selectable rows on the current screen.
    pub(super) fn nav_len(&self) -> usize {
        match self.app.screen.navigation.current_kind() {
            ScreenKind::Dashboard => DASHBOARD_ENTRIES.len(),
            kind if kind.has_data() => self.current_data().row_count().max(1),
            _ => 1,
        }
    }

    pub(super) fn goto(&mut self, kind: ScreenKind) {
        self.app.screen.navigation.navigate_to(kind);
        self.app.screen.settings_scroll = 0;
        self.request_data(kind);
    }

    pub(super) fn go_back(&mut self) -> bool {
        if self.app.screen.navigation.go_back() {
            let kind = self.app.screen.navigation.current_kind();
            self.app.screen.settings_scroll = 0;
            self.request_data(kind);
            true
        } else {
            false
        }
    }
}

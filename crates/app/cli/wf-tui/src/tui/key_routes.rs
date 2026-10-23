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
use tui_core::keymap::{KeyAction as TabKeyAction, KeymapContext};
use tui_core::screen_data::screen_for_digit;

impl TuiApp {
    pub(super) async fn handle_key(&mut self, key: Key) -> CliResult<LoopAction> {
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
            return self.handle_overlay_key(key).await;
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
            // Tab chords are intercepted before the session sees them so a
            // tab switch never leaks into the composer or the overlays.
            if let Some(action) = self.keymap.resolve(KeymapContext::Global, key) {
                let tab_handled = match action {
                    TabKeyAction::TabNew => {
                        self.new_tab().await;
                        self.app
                            .notice
                            .set(format!("tab {} opened", self.sessions.active_index() + 1));
                        true
                    }
                    TabKeyAction::TabClose => {
                        let index = self.sessions.active_index();
                        self.close_tab(index).await;
                        true
                    }
                    TabKeyAction::TabNext => {
                        self.sessions.next(self.now_ms());
                        self.save_tabs();
                        true
                    }
                    TabKeyAction::TabPrev => {
                        self.sessions.prev(self.now_ms());
                        self.save_tabs();
                        true
                    }
                    TabKeyAction::TabPick(slot) => {
                        let index = usize::from(slot).saturating_sub(1);
                        if self.sessions.switch(index, self.now_ms()) {
                            self.save_tabs();
                            true
                        } else {
                            self.app.notice.set(format!("no tab {slot}"));
                            true
                        }
                    }
                    TabKeyAction::TabPin => {
                        self.toggle_pin_active();
                        true
                    }
                    TabKeyAction::TabMovePrev => {
                        self.move_active(-1);
                        true
                    }
                    TabKeyAction::TabMoveNext => {
                        self.move_active(1);
                        true
                    }
                    TabKeyAction::TabParent => {
                        if !self.sessions.focus_parent(self.now_ms()) {
                            self.app.notice.set("no parent tab".to_string());
                        } else {
                            self.save_tabs();
                        }
                        true
                    }
                    TabKeyAction::TabChild => {
                        if !self.sessions.focus_child_first(self.now_ms()) {
                            self.app.notice.set("no child tab".to_string());
                        } else {
                            self.save_tabs();
                        }
                        true
                    }
                    TabKeyAction::TabChildNext => {
                        if !self.sessions.focus_child_next(self.now_ms()) {
                            self.app.notice.set("no child tab".to_string());
                        } else {
                            self.save_tabs();
                        }
                        true
                    }
                    TabKeyAction::TabChildPrev => {
                        if !self.sessions.focus_child_prev(self.now_ms()) {
                            self.app.notice.set("no child tab".to_string());
                        } else {
                            self.save_tabs();
                        }
                        true
                    }
                    TabKeyAction::SessionList => {
                        self.open_session_list();
                        true
                    }
                    _ => false,
                };
                if tab_handled {
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
            }
            // The session outcome is collected first so the shell actions
            // (new tab, session list, screen switch) can run without the
            // session borrow being alive.
            let session_outcome = self.sessions.active_mut().map(|session| {
                let action = session.handle_key(key);
                (
                    action,
                    session.take_tool_viewer(),
                    session.take_pending_copy(),
                    session.take_copy_notice(),
                )
            });
            if let Some((action, viewer, copy, notice)) = session_outcome {
                match action {
                    InteractiveAction::Continue => {}
                    InteractiveAction::Exit => {
                        self.interactive_exit = true;
                        return Ok(LoopAction::Continue);
                    }
                    InteractiveAction::ShowHelp => {
                        self.modals.push(Box::new(HelpModal::with_overrides(
                            self.keymap_overrides.clone(),
                        )));
                    }
                    InteractiveAction::NewSession => {
                        self.new_tab().await;
                    }
                    InteractiveAction::OpenSessionList => {
                        self.open_session_list();
                    }
                    InteractiveAction::OpenScreen(kind) => {
                        self.goto(kind);
                    }
                }
                if let Some(request) = viewer {
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
                if let Some(copy) = copy {
                    self.pending_copy = Some(copy);
                }
                if let Some(notice) = notice {
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

        // Shared actions resolve through the keymap (user overrides first,
        // then builtin defaults); screen-specific keys fall through to the
        // hard-coded match below.
        match self
            .keymap
            .resolve(tui_core::keymap::KeymapContext::List, key)
        {
            Some(tui_core::keymap::KeyAction::Help) => {
                self.modals.push(Box::new(HelpModal::with_overrides(
                    self.keymap_overrides.clone(),
                )));
                return Ok(LoopAction::Continue);
            }
            Some(tui_core::keymap::KeyAction::Back) => {
                if !self.go_back() {
                    return Ok(LoopAction::Quit);
                }
                return Ok(LoopAction::Continue);
            }
            Some(tui_core::keymap::KeyAction::Quit) => {
                return Ok(LoopAction::Quit);
            }
            Some(tui_core::keymap::KeyAction::MoveNext) => {
                let len = self.nav_len();
                self.app.screen.navigation.select_next(len);
                self.request_detail_for_current();
                return Ok(LoopAction::Continue);
            }
            Some(tui_core::keymap::KeyAction::MovePrev) => {
                let len = self.nav_len();
                self.app.screen.navigation.select_prev(len);
                self.request_detail_for_current();
                return Ok(LoopAction::Continue);
            }
            _ => {}
        }

        match key.code {
            CKey::Char(c) if c.is_ascii_digit() && !key.ctrl && !key.alt => {
                if let Some(kind) = screen_for_digit(c) {
                    self.goto(kind);
                } else if c == '7' {
                    self.overlay = OverlayMode::SearchPalette;
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                }
            }
            CKey::PageUp => {
                if self.scroll_detail(-10) {
                    return Ok(LoopAction::Continue);
                }
            }
            CKey::PageDown => {
                if self.scroll_detail(10) {
                    return Ok(LoopAction::Continue);
                }
            }
            CKey::Enter => match self.app.screen.navigation.current_kind() {
                ScreenKind::Dashboard => {
                    let idx = self.app.screen.navigation.selected();
                    if let Some(kind) = DASHBOARD_ENTRIES.get(idx).copied() {
                        self.goto(kind);
                    }
                }
                ScreenKind::Executions => {
                    if self.app.screen.exec_view == crate::screens::ExecView::Overview {
                        let rows_len = self.display_data().row_count();
                        let selected = self
                            .app
                            .screen
                            .navigation
                            .selected()
                            .min(rows_len.saturating_sub(1));
                        let id = match self.display_data() {
                            ScreenData::Executions(rows) => {
                                rows.get(selected).map(|r| r.id.clone())
                            }
                            _ => None,
                        };
                        if let Some(id) = id {
                            self.goto(ScreenKind::Interactive);
                            self.pending_replay = Some(id);
                        }
                    } else {
                        self.open_selected_exec_viewer();
                    }
                }
                ScreenKind::Workflow => {
                    self.open_selected_workflow_viewer();
                }
                ScreenKind::Checkpoints => {
                    self.open_selected_checkpoint_detail();
                }
                ScreenKind::Catalog
                | ScreenKind::Git
                | ScreenKind::FileWorkspace
                | ScreenKind::FileTimeline
                | ScreenKind::FileSync
                | ScreenKind::EditSessions
                | ScreenKind::FileApprovals
                | ScreenKind::Conflicts => {
                    self.open_selected_cached_detail();
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
            CKey::Char('c')
                if self.app.screen.navigation.current_kind() == ScreenKind::Checkpoints =>
            {
                self.create_checkpoint_for_selected();
            }
            CKey::Char('G')
                if self.app.screen.navigation.current_kind() == ScreenKind::Checkpoints =>
            {
                self.delete_checkpoints_for_selected_entity();
            }
            CKey::Char('s')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.submit_file_workspace_for_review();
            }
            CKey::Char('m')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.merge_file_workspace_to_feature();
            }
            CKey::Char('u')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.undo_file_workspace_edit();
            }
            CKey::Char('g')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.run_file_workspace_gc();
            }
            CKey::Char('i')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.rebuild_file_workspace_index();
            }
            CKey::Char('b')
                if self.app.screen.navigation.current_kind() == ScreenKind::EditSessions =>
            {
                self.begin_edit_session();
            }
            CKey::Char('X')
                if self.app.screen.navigation.current_kind() == ScreenKind::EditSessions =>
            {
                self.rollback_selected_edit_session();
            }
            CKey::Char('r')
                if key.ctrl
                    && !key.alt
                    && self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.redo_file_workspace_edit();
            }
            CKey::Char('r') => {
                let kind = self.app.screen.navigation.current_kind();
                self.invalidate(kind);
                self.request_detail_for_current();
                self.app
                    .notice
                    .set(format!("Refreshing {}...", kind.title()));
            }
            CKey::Char('[')
                if matches!(
                    self.app.screen.navigation.current_kind(),
                    ScreenKind::Executions | ScreenKind::Workflow | ScreenKind::Catalog
                ) =>
            {
                self.cycle_detail_view(false);
            }
            CKey::Char(']')
                if matches!(
                    self.app.screen.navigation.current_kind(),
                    ScreenKind::Executions | ScreenKind::Workflow | ScreenKind::Catalog
                ) =>
            {
                self.cycle_detail_view(true);
            }
            CKey::Tab
                if !key.ctrl
                    && !key.alt
                    && self.app.screen.navigation.current_kind() == ScreenKind::Catalog =>
            {
                self.cycle_detail_view(!key.shift);
            }
            CKey::Char('/')
                if !key.ctrl
                    && !key.alt
                    && self.app.screen.navigation.current_kind().has_data() =>
            {
                self.prompt_list_filter();
            }
            CKey::Char('X')
                if !key.ctrl
                    && !key.alt
                    && self.app.screen.navigation.current_kind().has_data() =>
            {
                if self.app.screen.list_filter.is_empty() {
                    self.app.notice.set("No active filter.".to_string());
                } else {
                    self.app.screen.list_filter.clear();
                    self.app.screen.detail_scroll = 0;
                    self.request_detail_for_current();
                    self.app.notice.set("Filter cleared.".to_string());
                }
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
            CKey::Char('t')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.open_selected_file_timeline();
            }
            CKey::Char('Y')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.open_file_sync();
            }
            CKey::Char('e')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileSync =>
            {
                self.export_file_sync();
            }
            CKey::Char('p')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileSync =>
            {
                self.import_file_sync();
            }
            CKey::Char('b')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileSync =>
            {
                self.rebase_file_sync();
            }
            CKey::Char('M')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.merge_file_workspace_to_main();
            }
            CKey::Char('R')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileWorkspace =>
            {
                self.rollback_file_workspace_feature();
            }
            CKey::Char('o')
                if self.app.screen.navigation.current_kind() == ScreenKind::Conflicts =>
            {
                self.resolve_selected_conflict_side(true);
            }
            CKey::Char('t')
                if self.app.screen.navigation.current_kind() == ScreenKind::Conflicts =>
            {
                self.resolve_selected_conflict_side(false);
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
            CKey::Char('y')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileApprovals =>
            {
                self.approve_selected_approval();
            }
            CKey::Char('n')
                if self.app.screen.navigation.current_kind() == ScreenKind::FileApprovals =>
            {
                self.reject_selected_approval();
            }
            CKey::Char('m')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.pick_default_model();
            }
            CKey::Char('o')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.toggle_mouse_capture();
            }
            CKey::Char('s')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.toggle_session_scrollbar();
            }
            CKey::Char('b')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.toggle_sidebar();
            }
            CKey::Char('c')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.cycle_copy_target();
            }
            CKey::Char('e')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.toggle_tool_output();
            }
            CKey::Char('l')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.cycle_tool_level();
            }
            CKey::Char('+')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.step_preview_budget(true);
            }
            CKey::Char('-')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.step_preview_budget(false);
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }

    pub(super) async fn handle_overlay_key(&mut self, key: Key) -> CliResult<LoopAction> {
        if self.overlay == OverlayMode::SearchPalette {
            return Ok(self.handle_search_palette_key(key));
        }
        if self.overlay == OverlayMode::SessionList {
            return self.handle_session_list_key(key).await;
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
            CKey::End if self.overlay == OverlayMode::History && !key.alt => {
                self.history.jump_to_latest();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            CKey::Home if self.overlay == OverlayMode::History && key.ctrl && !key.alt => {
                if let Some(before) = self.history.start_earlier() {
                    self.request_history_page(Some(before));
                } else {
                    self.history.jump_to_oldest();
                }
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

    async fn handle_session_list_key(&mut self, key: Key) -> CliResult<LoopAction> {
        let len = self.sessions.len();
        match key.code {
            CKey::Esc => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Enter => {
                let index = self.session_list.selected().min(len.saturating_sub(1));
                self.overlay = OverlayMode::None;
                if self.sessions.switch(index, self.now_ms()) {
                    self.save_tabs();
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('j') | CKey::Down if !key.ctrl && !key.alt => {
                self.session_list.move_next(len);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('k') | CKey::Up if !key.ctrl && !key.alt => {
                self.session_list.move_prev(len);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('p') if !key.ctrl && !key.alt => {
                let index = self.session_list.selected();
                if let Some(pinned) = self.sessions.toggle_pin(index) {
                    self.app.notice.set(if pinned {
                        format!("tab {} pinned", index + 1)
                    } else {
                        format!("tab {} unpinned", index + 1)
                    });
                    self.save_tabs();
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('d') if !key.ctrl && !key.alt => {
                let index = self.session_list.selected();
                self.close_tab(index).await;
                self.session_list
                    .open(self.sessions.active_index(), self.sessions.len());
                if self.sessions.is_empty() {
                    self.overlay = OverlayMode::None;
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('x') if !key.ctrl && !key.alt => {
                let Some(live) = self.live_execution_ids() else {
                    self.app
                        .notice
                        .set("no executions snapshot yet".to_string());
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                };
                let pruned = self.prune_closed_tabs(&live).await;
                self.session_list
                    .open(self.sessions.active_index(), self.sessions.len());
                self.app.notice.set(if pruned == 0 {
                    "no closed tabs to prune".to_string()
                } else {
                    format!("pruned {pruned} closed tab(s)")
                });
                if self.sessions.is_empty() {
                    self.overlay = OverlayMode::None;
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char(c) if c.is_ascii_digit() && !key.ctrl && !key.alt => {
                if let Some(digit) = c.to_digit(10) {
                    let index = (digit as usize).saturating_sub(1);
                    self.session_list.select(index, len);
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                }
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
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

    /// Scroll the executions/workflow/catalog detail pane. Reports whether
    /// the current screen owns a detail pane so callers can fall through.
    fn scroll_detail(&mut self, delta: isize) -> bool {
        if !matches!(
            self.app.screen.navigation.current_kind(),
            ScreenKind::Executions | ScreenKind::Workflow | ScreenKind::Catalog
        ) {
            return false;
        }
        if delta >= 0 {
            self.app.screen.detail_scroll =
                self.app.screen.detail_scroll.saturating_add(delta as usize);
        } else {
            self.app.screen.detail_scroll = self
                .app
                .screen
                .detail_scroll
                .saturating_sub(delta.unsigned_abs());
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        true
    }

    /// Cycle the executions/workflow subview and fetch its detail lines.
    /// On the catalog screen the same keys cycle the kind tab instead.
    fn cycle_detail_view(&mut self, forward: bool) {
        match self.app.screen.navigation.current_kind() {
            ScreenKind::Executions => {
                let view = self.app.screen.exec_view;
                self.app.screen.exec_view = if forward { view.next() } else { view.prev() };
                self.request_detail_for_current();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            ScreenKind::Workflow => {
                let view = self.app.screen.workflow_view;
                self.app.screen.workflow_view = if forward { view.next() } else { view.prev() };
                self.request_detail_for_current();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            ScreenKind::Catalog => {
                self.cycle_catalog_tab(forward);
            }
            _ => {}
        }
    }

    /// Cycle the catalog kind tab, wrapping around the five groups.
    fn cycle_catalog_tab(&mut self, forward: bool) {
        use tui_core::screen_data::catalog_tab_from_index;
        let total = tui_core::screen_data::CatalogKind::ALL.len() + 1;
        let current = tui_core::screen_data::catalog_tab_index(self.app.screen.catalog_tab);
        let next = if forward {
            (current + 1) % total
        } else {
            (current + total - 1) % total
        };
        self.app
            .screen
            .switch_catalog_tab(catalog_tab_from_index(next));
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Number of selectable rows on the current screen.
    pub(super) fn nav_len(&self) -> usize {
        match self.app.screen.navigation.current_kind() {
            kind if kind.has_data() => self.display_data().row_count().max(1),
            _ => 1,
        }
    }

    pub(super) fn goto(&mut self, kind: ScreenKind) {
        self.app.screen.navigation.navigate_to(kind);
        self.app.screen.settings_scroll = 0;
        self.app.screen.detail_scroll = 0;
        self.app.screen.list_filter.clear();
        if kind == ScreenKind::Catalog {
            self.app.screen.reset_catalog();
        }
        self.request_data(kind);
        self.request_detail_for_current();
    }

    pub(super) fn go_back(&mut self) -> bool {
        if self.app.screen.navigation.go_back() {
            let kind = self.app.screen.navigation.current_kind();
            self.app.screen.settings_scroll = 0;
            self.app.screen.detail_scroll = 0;
            self.app.screen.list_filter.clear();
            self.request_data(kind);
            self.request_detail_for_current();
            true
        } else {
            false
        }
    }
}

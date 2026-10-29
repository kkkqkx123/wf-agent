//! Non-interactive (normal mode) key handling: settings scroll, shared
//! keymap actions and screen-specific bindings.

use crate::error::CliResult;
use crate::keymap::{CKey, Key};
use crate::modal::HelpModal;
use crate::overlay::{LoopAction, OverlayMode};
use crate::screens::{ScreenData, ScreenKind};
use tui_core::keymap::KeymapContext;

use super::input_classify::next_filter;
use super::{TuiApp, DASHBOARD_ENTRIES};
use tui_core::screen_data::screen_for_digit;

impl TuiApp {
    pub(super) async fn handle_normal_key(&mut self, key: Key) -> CliResult<LoopAction> {
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
        // then registry bindings); screen-specific keys fall through to the
        // hard-coded match below.
        if let Some(command) = self.keymap.resolve(KeymapContext::List, key) {
            match command.id {
                "app.help" => {
                    self.modals.push(Box::new(HelpModal::new()));
                    return Ok(LoopAction::Continue);
                }
                "ui.back" => {
                    if !self.go_back() {
                        return Ok(LoopAction::Quit);
                    }
                    return Ok(LoopAction::Continue);
                }
                "app.quit" => {
                    return Ok(LoopAction::Quit);
                }
                "ui.move_next" => {
                    let len = self.nav_len();
                    self.app.screen.navigation.select_next(len);
                    self.request_detail_for_current();
                    return Ok(LoopAction::Continue);
                }
                "ui.move_prev" => {
                    let len = self.nav_len();
                    self.app.screen.navigation.select_prev(len);
                    self.request_detail_for_current();
                    return Ok(LoopAction::Continue);
                }
                _ => {}
            }
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
            CKey::Char('U')
                if self.app.screen.navigation.current_kind() == ScreenKind::Checkpoints =>
            {
                self.undo_last_input_for_selected();
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
            CKey::Char('y')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.toggle_copy_on_select();
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
            CKey::Char('t')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.cycle_double_escape();
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
}

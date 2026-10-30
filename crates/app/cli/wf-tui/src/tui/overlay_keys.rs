//! Overlay-specific key handling: search palette, session list, session
//! tree, history selection and the sidebar.

use crate::error::CliResult;
use crate::keymap::{CKey, Key};
use crate::overlay::{LoopAction, OverlayMode};

use super::{TuiApp, DASHBOARD_ENTRIES};
use tui_core::panel_select::Motion;
use tui_core::screen_data::screen_for_digit;

impl TuiApp {
    pub(super) async fn handle_overlay_key(&mut self, key: Key) -> CliResult<LoopAction> {
        if self.overlay == OverlayMode::SearchPalette {
            return Ok(self.handle_search_palette_key(key));
        }
        if self.overlay == OverlayMode::CommandPalette {
            return self.handle_command_palette_key(key).await;
        }
        if self.overlay == OverlayMode::SessionList {
            return self.handle_session_list_key(key).await;
        }
        if self.overlay == OverlayMode::SessionTree {
            return Ok(self.handle_session_tree_key(key).await);
        }
        if self.overlay == OverlayMode::History {
            if let Some(action) = self.handle_history_selection_key(key) {
                return Ok(action);
            }
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

    /// History pager selection keys: Ctrl+A selects every loaded row,
    /// Shift+motion extends the logical cursor, Ctrl+Y copies the selection
    /// and Esc clears it before the overlay close key runs.
    fn handle_history_selection_key(&mut self, key: Key) -> Option<LoopAction> {
        if key.ctrl && !key.alt && matches!(key.code, CKey::Char('y') | CKey::Char('Y')) {
            match self.history.selection_text() {
                Some(text) => {
                    self.queue_selection_copy(text);
                    if self.clear_selection_after_copy() {
                        self.history.clear_selection();
                    }
                }
                None => self.app.notice.set("copy: no selection".to_string()),
            }
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return Some(LoopAction::Continue);
        }
        if key.ctrl && !key.alt && key.code == CKey::Char('a') {
            self.history.select_all();
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return Some(LoopAction::Continue);
        }
        if key.code == CKey::Esc && self.history.has_selection() {
            self.history.clear_selection();
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return Some(LoopAction::Continue);
        }
        if key.shift && !key.ctrl && !key.alt {
            let motion = match key.code {
                CKey::Left => Some(Motion::Left),
                CKey::Right => Some(Motion::Right),
                CKey::Up => Some(Motion::Up),
                CKey::Down => Some(Motion::Down),
                CKey::Home => Some(Motion::LineStart),
                CKey::End => Some(Motion::LineEnd),
                _ => None,
            }?;
            if self.history.extend_motion(motion) {
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            return Some(LoopAction::Continue);
        }
        None
    }

    /// Session tree browser keys. Arrows move (every printable character
    /// types into the search, so there are no plain-letter bindings here):
    /// Enter branches from the selected node, Ctrl+Y copies its text,
    /// Ctrl+D/T/A/R select the four filters, Ctrl+O cycles them, Ctrl+B
    /// toggles the bookmark, Left/Right folds sections. Esc clears an
    /// active search first and closes the overlay otherwise.
    async fn handle_session_tree_key(&mut self, key: Key) -> LoopAction {
        let plain = !key.ctrl && !key.alt;
        let ctrl = key.ctrl && !key.alt;
        // While the restore-scope selector is open, only the scope keys
        // and Esc are meaningful; every other key is swallowed so the
        // search box cannot eat the digits.
        if self.session_tree.restore_prompt() {
            match key.code {
                CKey::Esc if plain => {
                    self.session_tree.close_restore_prompt();
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                }
                CKey::Char('1') if plain => {
                    self.session_tree.close_restore_prompt();
                    self.branch_from_tree_with_scope(crate::session_tree::RestoreScope::History)
                        .await;
                }
                CKey::Char('2') if plain => {
                    self.session_tree.close_restore_prompt();
                    self.branch_from_tree_with_scope(crate::session_tree::RestoreScope::Workspace)
                        .await;
                }
                CKey::Char('3') if plain => {
                    self.session_tree.close_restore_prompt();
                    self.branch_from_tree_with_scope(crate::session_tree::RestoreScope::Both)
                        .await;
                }
                _ => {}
            }
            return LoopAction::Continue;
        }
        match key.code {
            CKey::Esc if plain => {
                if !self.session_tree.search_query().is_empty() {
                    self.session_tree.clear_search();
                } else {
                    self.session_tree.close();
                    self.overlay = OverlayMode::None;
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Down if plain => {
                self.session_tree.move_next();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Up if plain => {
                self.session_tree.move_prev();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::PageDown if plain => {
                self.session_tree.page_by(10);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::PageUp if plain => {
                self.session_tree.page_by(-10);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Home if plain => {
                self.session_tree.jump_to_first();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::End if plain => {
                self.session_tree.jump_to_last();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Left | CKey::Right if plain => {
                self.session_tree.toggle_fold();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('e') | CKey::Char('E') if ctrl => {
                self.session_tree.expand_all();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('g') | CKey::Char('G') if ctrl => {
                self.session_tree.collapse_all();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Enter if plain => {
                if self
                    .session_tree
                    .selected_node()
                    .and_then(|node| node.checkpoint_id.as_ref())
                    .is_none()
                {
                    self.app.notice.set(
                        "tree: no checkpoint at or before this point yet — send a turn first"
                            .to_string(),
                    );
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                } else {
                    self.session_tree.open_restore_prompt();
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                }
            }
            CKey::Char('y') | CKey::Char('Y') if ctrl => {
                match self.session_tree.selected_copy_text() {
                    Some(text) => {
                        self.queue_selection_copy(text);
                    }
                    None => self.app.notice.set("tree: nothing to copy".to_string()),
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('d') | CKey::Char('D') if ctrl => {
                self.session_tree
                    .set_filter(crate::session_tree::TreeFilter::Default);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('t') | CKey::Char('T') if ctrl => {
                self.session_tree
                    .set_filter(crate::session_tree::TreeFilter::NoTools);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('a') | CKey::Char('A') if ctrl => {
                self.session_tree
                    .set_filter(crate::session_tree::TreeFilter::All);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('r') | CKey::Char('R') if ctrl => {
                self.session_tree
                    .set_filter(crate::session_tree::TreeFilter::Bookmarked);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('o') | CKey::Char('O') if ctrl => {
                self.session_tree.cycle_filter();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('b') | CKey::Char('B') if ctrl => {
                self.toggle_tree_bookmark();
            }
            CKey::Backspace if plain => {
                self.session_tree.search_backspace();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char(c) if plain && !c.is_control() => {
                self.session_tree.search_push(c);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            _ => {}
        }
        LoopAction::Continue
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

    /// Shell command palette keys. The overlay owns its filter, so every
    /// printable character types into the palette and never reaches the
    /// session composer. `Ctrl-P`/`Esc` close back to the untouched draft;
    /// `Enter` runs the selected command with the typed argument.
    async fn handle_command_palette_key(&mut self, key: Key) -> CliResult<LoopAction> {
        match key.code {
            CKey::Esc => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('p') | CKey::Char('P') if key.ctrl && !key.alt => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Enter => {
                let Some(command) = self.command_palette.selected_command() else {
                    self.app.notice.set("No matching command.".to_string());
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                };
                let argument = self.command_palette.argument();
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return self.run_palette_command(command, &argument).await;
            }
            CKey::Backspace if !key.ctrl && !key.alt => {
                self.command_palette.pop();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('u') | CKey::Char('U') if key.ctrl && !key.alt => {
                self.command_palette.clear();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('j') if key.ctrl || key.alt => {
                self.command_palette.move_next();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char('k') if key.ctrl || key.alt => {
                self.command_palette.move_prev();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Down => {
                self.command_palette.move_next();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Up => {
                self.command_palette.move_prev();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::PageDown => {
                self.command_palette.page_down();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::PageUp => {
                self.command_palette.page_up();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.command_palette.push_char(c);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }

    async fn handle_session_list_key(&mut self, key: Key) -> CliResult<LoopAction> {
        // Tab toggles the tabs/recent panes everywhere in this overlay.
        if key.code == CKey::Tab && !key.ctrl && !key.alt {
            self.session_list.toggle_pane();
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return Ok(LoopAction::Continue);
        }
        if self.session_list.pane() == crate::session_list::SessionListPane::Recent {
            return self.handle_recent_list_key(key).await;
        }
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

    /// Recent-sessions pane keys. Arrows move and Enter continues the session
    /// (switching to its tab when already open, otherwise opening a new tab
    /// whose history seeds the next turn). Every printable character filters
    /// over id, title and status (`"phrase"` matches exactly), so letter
    /// commands live on Ctrl: Ctrl+D deletes the selected session in two
    /// steps (arm, then repeat to confirm), Ctrl+R renames it to the filter
    /// text, Ctrl+N toggles the named-only filter. Esc cancels an armed
    /// delete first, then clears the filter, then closes.
    async fn handle_recent_list_key(&mut self, key: Key) -> CliResult<LoopAction> {
        let plain = !key.ctrl && !key.alt;
        let ctrl = key.ctrl && !key.alt;
        // Delete, rename and named-filter on Ctrl so typing stays a filter.
        if ctrl {
            match key.code {
                CKey::Char('d') | CKey::Char('D') => {
                    let Some(id) = self.session_list.recent_selected_id() else {
                        self.app.notice.set("recent: nothing selected".to_string());
                        self.pending_scope.request(crate::redraw::RedrawScope::Full);
                        return Ok(LoopAction::Continue);
                    };
                    if self.session_list.confirm_delete(&id) {
                        self.delete_recent_session().await;
                    } else {
                        let short = crate::screens::short_id(&id);
                        self.session_list.arm_delete(id);
                        self.app.notice.set(format!(
                            "press Ctrl+D again to confirm delete {short}, Esc cancels"
                        ));
                        self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    }
                    return Ok(LoopAction::Continue);
                }
                CKey::Char('r') | CKey::Char('R') => {
                    self.rename_recent_session().await;
                    return Ok(LoopAction::Continue);
                }
                CKey::Char('n') | CKey::Char('N') => {
                    self.session_list.toggle_named_only();
                    self.app.notice.set(if self.session_list.named_only() {
                        "recent: named sessions only (Ctrl+N for all)".to_string()
                    } else {
                        "recent: all sessions".to_string()
                    });
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
                _ => {}
            }
        }
        match key.code {
            CKey::Esc if plain => {
                if self.session_list.cancel_pending_delete() {
                    self.app.notice.set("delete cancelled".to_string());
                } else if !self.session_list.recent_filter().is_empty() {
                    self.session_list.clear_recent_filter();
                } else {
                    self.overlay = OverlayMode::None;
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Enter if plain => {
                if self.session_list.recent_is_empty() {
                    self.app
                        .notice
                        .set("no recent sessions yet — send a turn first".to_string());
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
                let Some(id) = self.session_list.recent_selected_id() else {
                    self.app.notice.set("recent: nothing selected".to_string());
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                };
                let short = crate::screens::short_id(&id);
                self.overlay = OverlayMode::None;
                if let Some(index) = self.sessions.index_of_execution(&id) {
                    self.sessions.switch(index, self.now_ms());
                    self.save_tabs();
                    self.app
                        .notice
                        .set(format!("switched to tab for {short} — history continues"));
                } else {
                    self.open_tab_for_session(id, None, None).await;
                    self.app.notice.set(format!(
                        "opened {short} — history loaded, continue chatting"
                    ));
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Down if plain => {
                self.session_list.recent_move_next();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Up if plain => {
                self.session_list.recent_move_prev();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::PageDown if plain => {
                self.session_list.recent_page_by(10);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::PageUp if plain => {
                self.session_list.recent_page_by(-10);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Home if plain => {
                self.session_list.recent_jump_to_first();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::End if plain => {
                self.session_list.recent_jump_to_last();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Backspace if plain => {
                self.session_list.recent_filter_backspace();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            CKey::Char(c) if plain && !c.is_control() => {
                self.session_list.recent_filter_push(c);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }
}

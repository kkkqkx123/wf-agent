//! Top-level key dispatch for the TUI shell: global keys, modal and
//! overlay routing, interactive-session delegation.

use crate::error::CliResult;
use crate::interactive::InteractiveAction;
use crate::keymap::{CKey, Key};
use crate::modal::HelpModal;
use crate::overlay::{LoopAction, OverlayMode};
use crate::screens::ScreenKind;

use super::signals::SUSPEND_PENDING;
use super::TuiApp;
use tui_core::keymap::{KeyAction, KeymapContext};

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
                                if self.clear_selection_after_copy() {
                                    self.modals.clear_selection();
                                }
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

        if let Some(action) = self.handle_leader_key(key).await? {
            return Ok(action);
        }

        // Shell command palette: global, owns its filter, leaves the
        // session composer draft untouched. Resolved through the keymap so
        // user overrides apply; checked before the session sees the key so
        // the chord never leaks into the composer or anchor jumps.
        if self.keymap.resolve_action(KeymapContext::Global, key) == Some(KeyAction::Palette) {
            self.open_command_palette();
            return Ok(LoopAction::Continue);
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
            if let Some(command) = self.keymap.resolve(KeymapContext::Global, key) {
                let tab_handled = match command.action {
                    Some(KeyAction::TabNew) => {
                        self.new_tab().await;
                        self.app
                            .notice
                            .set(format!("tab {} opened", self.sessions.active_index() + 1));
                        true
                    }
                    Some(KeyAction::TabClose) => {
                        let index = self.sessions.active_index();
                        self.close_tab(index).await;
                        true
                    }
                    Some(KeyAction::TabNext) => {
                        self.sessions.next(self.now_ms());
                        self.save_tabs();
                        true
                    }
                    Some(KeyAction::TabPrev) => {
                        self.sessions.prev(self.now_ms());
                        self.save_tabs();
                        true
                    }
                    Some(KeyAction::TabPick(slot)) => {
                        let index = usize::from(slot).saturating_sub(1);
                        if self.sessions.switch(index, self.now_ms()) {
                            self.save_tabs();
                            true
                        } else {
                            self.app.notice.set(format!("no tab {slot}"));
                            true
                        }
                    }
                    Some(KeyAction::TabPin) => {
                        self.toggle_pin_active();
                        true
                    }
                    Some(KeyAction::TabMovePrev) => {
                        self.move_active(-1);
                        true
                    }
                    Some(KeyAction::TabMoveNext) => {
                        self.move_active(1);
                        true
                    }
                    Some(KeyAction::TabParent) => {
                        if !self.sessions.focus_parent(self.now_ms()) {
                            self.app.notice.set("no parent tab".to_string());
                        } else {
                            self.save_tabs();
                        }
                        true
                    }
                    Some(KeyAction::TabChild) => {
                        if !self.sessions.focus_child_first(self.now_ms()) {
                            self.app.notice.set("no child tab".to_string());
                        } else {
                            self.save_tabs();
                        }
                        true
                    }
                    Some(KeyAction::TabChildNext) => {
                        if !self.sessions.focus_child_next(self.now_ms()) {
                            self.app.notice.set("no child tab".to_string());
                        } else {
                            self.save_tabs();
                        }
                        true
                    }
                    Some(KeyAction::TabChildPrev) => {
                        if !self.sessions.focus_child_prev(self.now_ms()) {
                            self.app.notice.set("no child tab".to_string());
                        } else {
                            self.save_tabs();
                        }
                        true
                    }
                    Some(KeyAction::SessionList) => {
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
                        self.modals.push(Box::new(HelpModal::new()));
                    }
                    InteractiveAction::NewSession => {
                        self.new_tab().await;
                    }
                    InteractiveAction::OpenSessionList => {
                        self.open_session_list();
                    }
                    InteractiveAction::OpenSessionTree => {
                        self.open_session_tree();
                    }
                    InteractiveAction::ForkLatest => {
                        self.branch_latest().await;
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

        return self.handle_normal_key(key).await;
    }
}

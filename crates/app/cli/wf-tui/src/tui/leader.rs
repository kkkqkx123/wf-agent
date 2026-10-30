//! Leader-key sequence handling and the commands it dispatches.

use crate::error::CliResult;
use crate::interactive::InteractiveAction;
use crate::keymap::Key;
use crate::modal::HelpModal;
use crate::overlay::LoopAction;
use crate::screens::ScreenKind;

use super::TuiApp;

impl TuiApp {
    /// Resolve a leader sequence: the leader opens a pending sequence, the
    /// following keys either complete a command, extend a prefix (showing
    /// the which-key hint) or miss (the sequence is dropped).
    pub(super) async fn handle_leader_key(&mut self, key: Key) -> CliResult<Option<LoopAction>> {
        let now = self.now_ms();
        if let Some(pending) = &self.pending_sequence {
            if pending.is_expired(now, self.sequence.timeout_ms()) {
                self.pending_sequence = None;
                self.app.notice.set("leader: timed out");
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
        }
        if self.pending_sequence.is_none() {
            if self.sequence.is_leader(key) && !key.alt {
                self.pending_sequence = Some(tui_core::command::PendingSequence::start(now));
                let hint = self.sequence.which_key(&[]);
                self.app.notice.set(hint);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(Some(LoopAction::Continue));
            }
            return Ok(None);
        }
        let keys = {
            let Some(pending) = self.pending_sequence.as_mut() else {
                return Ok(None);
            };
            pending.push(key);
            pending.keys().to_vec()
        };
        if let Some(command) = self.sequence.resolve(&keys) {
            self.pending_sequence = None;
            self.app.notice.clear();
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return Ok(Some(self.run_leader_command(command).await?));
        }
        if self.sequence.is_prefix(&keys) {
            let hint = self.sequence.which_key(&keys);
            self.app.notice.set(hint);
        } else {
            self.pending_sequence = None;
            self.app.notice.set(self.sequence.which_key(&keys));
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        Ok(Some(LoopAction::Continue))
    }

    /// Execute a command reached through the leader sequence.
    async fn run_leader_command(
        &mut self,
        command: &'static tui_core::command::CommandSpec,
    ) -> CliResult<LoopAction> {
        self.run_palette_command(command, "").await
    }

    /// Execute a command chosen in the shell command palette with the
    /// argument typed after the command word. Session commands run on the
    /// active session; the overlay is already closed by the caller.
    pub(super) async fn run_palette_command(
        &mut self,
        command: &'static tui_core::command::CommandSpec,
        argument: &str,
    ) -> CliResult<LoopAction> {
        match command.id {
            "app.quit" => {
                if self.app.screen.navigation.current_kind() == ScreenKind::Interactive {
                    self.interactive_exit = true;
                    Ok(LoopAction::Continue)
                } else {
                    Ok(LoopAction::Quit)
                }
            }
            "app.help" => {
                self.modals.push(Box::new(HelpModal::new()));
                Ok(LoopAction::Continue)
            }
            "command.palette" => {
                self.open_command_palette();
                Ok(LoopAction::Continue)
            }
            _ => {
                if self.sessions.active_mut().is_none() {
                    self.app
                        .notice
                        .set(format!("{} needs an active session", command.title));
                    return Ok(LoopAction::Continue);
                }
                // Blocking footer views own their keys: running a panel
                // command now would drop the pending approval or question.
                if self.sessions.active().is_some_and(|session| {
                    session.approval_active() || session.footer.question.is_some()
                }) {
                    self.app
                        .notice
                        .set("finish the pending approval or question first".to_string());
                    return Ok(LoopAction::Continue);
                }
                self.goto(ScreenKind::Interactive);
                let action = self
                    .sessions
                    .active_mut()
                    .map(|session| session.run_command(command, argument));
                match action {
                    Some(InteractiveAction::Exit) => self.interactive_exit = true,
                    Some(InteractiveAction::ShowHelp) => {
                        self.modals.push(Box::new(HelpModal::new()));
                    }
                    Some(InteractiveAction::NewSession) => self.new_tab().await,
                    Some(InteractiveAction::OpenSessionList) => self.open_session_list(),
                    Some(InteractiveAction::OpenSessionTree) => self.open_session_tree(),
                    Some(InteractiveAction::ForkLatest) => self.branch_latest().await,
                    Some(InteractiveAction::OpenScreen(kind)) => self.goto(kind),
                    Some(InteractiveAction::Continue) | None => {}
                }
                Ok(LoopAction::Continue)
            }
        }
    }
}

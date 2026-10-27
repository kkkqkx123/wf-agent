//! Terminal-side state synchronization for the full-screen TUI shell.
//!
//! Keeps the last reported title, progress flag and program status so the
//! event loop can call the terminal control plane only on changes.

use tui_terminal::terminal::control::TerminalControl;
use tui_terminal::terminal::guard::TerminalGuard;
use tui_terminal::terminal::state::{ProgramStatus, ProgramStatusState};

#[derive(Default)]
pub struct TerminalStateManager {
    title: Option<String>,
    progress: bool,
    last_progress_write: Option<std::time::Instant>,
    program_status_key: Option<String>,
}

impl TerminalStateManager {
    pub fn sync_title<C>(&mut self, guard: &mut TerminalGuard<C>, desired: &str)
    where
        C: TerminalControl,
    {
        if self.title.as_deref() == Some(desired) {
            return;
        }
        let _ = guard.control().set_title(desired);
        self.title = Some(desired.to_string());
    }

    pub fn sync_progress<C>(&mut self, guard: &mut TerminalGuard<C>, desired: bool, enabled: bool)
    where
        C: TerminalControl,
    {
        let desired = desired && enabled;
        let now = std::time::Instant::now();
        // Keepalive: re-issue the active sequence at most once per second
        // while processing so terminals that stop animating on silence keep
        // going.
        let keepalive = desired
            && self.progress
            && self
                .last_progress_write
                .map(|t| now.duration_since(t).as_secs_f32() >= 1.0)
                .unwrap_or(false);
        if self.progress == desired && !keepalive {
            return;
        }
        let _ = guard.control().set_progress(desired);
        self.progress = desired;
        self.last_progress_write = if desired { Some(now) } else { None };
    }

    pub fn sync_program_status<C>(&mut self, guard: &mut TerminalGuard<C>, desired: ProgramStatus)
    where
        C: TerminalControl,
    {
        let key = format!("{:?}:{:?}", desired.state, desired.message);
        if self.program_status_key.as_deref() == Some(key.as_str()) {
            return;
        }
        let _ = guard.control().set_program_status(desired);
        self.program_status_key = Some(key);
    }

    pub fn clear<C>(&mut self, guard: &mut TerminalGuard<C>)
    where
        C: TerminalControl,
    {
        if self.progress {
            let _ = guard.control().set_progress(false);
            self.progress = false;
        }
        let _ = guard.control().set_program_status(ProgramStatus::clear());
        self.program_status_key = Some("clear".to_string());
    }

    pub fn force_idle_program_status<C>(
        &mut self,
        guard: &mut TerminalGuard<C>,
        session_title: Option<&str>,
    ) where
        C: TerminalControl,
    {
        let status = ProgramStatus {
            state: ProgramStatusState::Idle,
            app: Some("wf".to_string()),
            kind: None,
            message: session_title.map(str::to_string),
        };
        self.sync_program_status(guard, status);
    }
}

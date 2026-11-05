//! Terminal-side state synchronization for the full-screen TUI shell.
//!
//! Keeps the last reported title, progress flag and program status so the
//! event loop can call the terminal control plane only on changes.

use tui_terminal::terminal::control::TerminalControl;
use tui_terminal::terminal::guard::TerminalGuard;
use tui_terminal::terminal::state::{ProgramStatus, ProgramStatusState};

/// Identity of a program status report for change detection. Every field
/// that reaches the wire participates so a report that only switches the
/// blocked kind (or the app label) is never swallowed as a duplicate.
fn program_status_key(status: &ProgramStatus) -> String {
    format!(
        "{:?}:{:?}:{:?}:{:?}",
        status.state, status.app, status.kind, status.message
    )
}

#[derive(Default)]
pub struct TerminalStateManager {
    title: Option<String>,
    progress: bool,
    last_progress_write: Option<std::time::Instant>,
    program_status_key: Option<String>,
    /// Live probe result for OSC 7501 support. Undecided keeps sending so
    /// paths that never probe behave exactly as before.
    program_status_supported: Option<bool>,
}

/// Resolve program status emission once at startup: an explicit
/// environment switch wins, otherwise the terminal is asked live.
pub fn resolve_program_status_support() -> bool {
    tui_terminal::terminal::state::program_status_override().unwrap_or_else(|| {
        tui_terminal::probe::probe_program_status_support(
            tui_terminal::probe::PROGRAM_STATUS_PROBE_TIMEOUT,
        )
    })
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

    /// Record the live probe result. Flipping to supported invalidates the
    /// key so the next sync re-emits instead of staying silent.
    pub fn set_program_status_support(&mut self, supported: bool) {
        self.program_status_supported = Some(supported);
        if supported {
            self.program_status_key = None;
        }
    }

    fn program_status_allowed(&self) -> bool {
        self.program_status_supported != Some(false)
    }

    pub fn sync_program_status<C>(&mut self, guard: &mut TerminalGuard<C>, desired: ProgramStatus)
    where
        C: TerminalControl,
    {
        let key = program_status_key(&desired);
        if self.program_status_key.as_deref() == Some(key.as_str()) {
            return;
        }
        if self.program_status_allowed() {
            let _ = guard.control().set_program_status(desired);
        }
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
        if self.program_status_allowed() {
            let _ = guard.control().set_program_status(ProgramStatus::clear());
        }
        self.program_status_key = Some(program_status_key(&ProgramStatus::clear()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use tui_terminal::terminal::control::FakeControl;
    use tui_terminal::terminal::state::ProgramStatusKind;

    fn blocked(kind: ProgramStatusKind) -> ProgramStatus {
        ProgramStatus {
            state: ProgramStatusState::Blocked,
            app: Some("wf".to_string()),
            kind: Some(kind),
            message: Some("dialog".to_string()),
        }
    }

    #[test]
    fn blocked_kind_switch_resends_with_same_message() {
        let mut manager = TerminalStateManager::default();
        let mut guard = TerminalGuard::new(FakeControl::default());
        manager.sync_program_status(&mut guard, blocked(ProgramStatusKind::Permission));
        assert_eq!(guard.control().ops.len(), 1);
        manager.sync_program_status(&mut guard, blocked(ProgramStatusKind::Permission));
        assert_eq!(guard.control().ops.len(), 1);
        manager.sync_program_status(&mut guard, blocked(ProgramStatusKind::Question));
        assert_eq!(guard.control().ops.len(), 2);
    }

    #[test]
    fn unsupported_terminal_suppresses_reports() {
        let mut manager = TerminalStateManager::default();
        let mut guard = TerminalGuard::new(FakeControl::default());
        manager.set_program_status_support(false);
        manager.sync_program_status(&mut guard, blocked(ProgramStatusKind::Permission));
        assert!(guard.control().ops.is_empty());
    }

    #[test]
    fn support_flip_flushes_the_current_report() {
        let mut manager = TerminalStateManager::default();
        let mut guard = TerminalGuard::new(FakeControl::default());
        manager.set_program_status_support(false);
        manager.sync_program_status(&mut guard, blocked(ProgramStatusKind::Permission));
        manager.set_program_status_support(true);
        manager.sync_program_status(&mut guard, blocked(ProgramStatusKind::Permission));
        assert_eq!(guard.control().ops.len(), 1);
    }

    #[test]
    fn clear_allows_the_next_report_through() {
        let mut manager = TerminalStateManager::default();
        let mut guard = TerminalGuard::new(FakeControl::default());
        manager.sync_program_status(&mut guard, blocked(ProgramStatusKind::Permission));
        manager.clear(&mut guard);
        assert_eq!(guard.control().ops.len(), 2);
        manager.sync_program_status(&mut guard, blocked(ProgramStatusKind::Permission));
        assert_eq!(guard.control().ops.len(), 3);
    }
}

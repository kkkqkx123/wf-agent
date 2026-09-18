//! External editor handoff: suspend and resume around editor calls.
//!
//! Opening an editor from the TUI must leave no terminal mode behind: the
//! handoff restores the terminal before the editor runs, then re-applies the
//! full TUI modes, re-queries geometry and forces one full redraw on return.

use super::terminal::{TerminalControl, TerminalGuard, TerminalModes};
use wf_cli_shared::CliResult;

/// Handoff state around one editor invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorHandoff {
    #[default]
    Idle,
    Suspended,
}

impl EditorHandoff {
    pub fn is_suspended(self) -> bool {
        matches!(self, EditorHandoff::Suspended)
    }
}

/// Restore the terminal before handing control to the editor.
pub fn suspend_for_editor<C: TerminalControl>(
    guard: &mut TerminalGuard<C>,
    state: &mut EditorHandoff,
) -> CliResult<()> {
    guard.restore()?;
    *state = EditorHandoff::Suspended;
    Ok(())
}

/// Re-apply the full TUI modes after the editor exits. Callers re-query
/// geometry and force a full redraw after this returns.
pub fn resume_after_editor<C: TerminalControl>(
    guard: &mut TerminalGuard<C>,
    modes: TerminalModes,
    state: &mut EditorHandoff,
) -> CliResult<()> {
    guard.enter(modes)?;
    *state = EditorHandoff::Idle;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::terminal::{FakeControl, TerminalGuard, TerminalModes};
    use super::*;

    #[test]
    fn suspend_then_resume_restores_full_modes() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard
            .enter(TerminalModes::TUI)
            .expect("enter works on fake control");
        let mut state = EditorHandoff::Idle;
        suspend_for_editor(&mut guard, &mut state).expect("suspend works");
        assert!(state.is_suspended());
        resume_after_editor(&mut guard, TerminalModes::TUI, &mut state).expect("resume works");
        assert_eq!(state, EditorHandoff::Idle);
    }
}

//! Terminal interaction facilities shared by the mini and full TUI forms.
//!
//! This module re-exports from focused sub-modules:
//! - Guard RAII state machine (`TerminalGuard`, `TerminalModes`, etc.)
//! - Stderr suppression (`TerminalStderrGuard`)
//! - SIGINT double-press tracker (`DoublePressTracker`)
//! - Terminal capability probing (`TerminalProbe`)

use std::fmt;
use std::io::{self, Write};

use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};

use crate::error::CliResult;

// ── Re-exports from sub-modules ────────────────────────────────────────

pub use crate::probe::{ColorSet, TerminalProbe};
pub use crate::sigint::{DoublePressTracker, PressOutcome, SIGINT_DOUBLE_PRESS_WINDOW};
pub use crate::stderr::TerminalStderrGuard;

// ── modes state machine ───────────────────────────────────────────────

/// Terminal mode switches tracked by [`TerminalGuard`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalModes {
    /// crossterm raw mode (no line buffering / echo).
    pub raw: bool,
    /// Alternate screen buffer (full TUI only).
    pub alt_screen: bool,
    /// Bracketed paste mode.
    pub bracketed_paste: bool,
    /// Cursor hidden.
    pub cursor_hidden: bool,
}

impl TerminalModes {
    /// All switches off (the restored baseline).
    pub const OFF: Self = Self {
        raw: false,
        alt_screen: false,
        bracketed_paste: false,
        cursor_hidden: false,
    };

    /// Typical mini session: inline viewport, no alt screen.
    pub const MINI: Self = Self {
        raw: true,
        alt_screen: false,
        bracketed_paste: true,
        cursor_hidden: false,
    };

    /// Typical full TUI session: alt screen + hidden cursor.
    pub const TUI: Self = Self {
        raw: true,
        alt_screen: true,
        bracketed_paste: true,
        cursor_hidden: true,
    };
}

/// Abstract terminal control plane (injectable for tests).
pub trait TerminalControl: fmt::Debug {
    fn enable_raw(&mut self) -> io::Result<()>;
    fn disable_raw(&mut self) -> io::Result<()>;
    fn enter_alt_screen(&mut self) -> io::Result<()>;
    fn leave_alt_screen(&mut self) -> io::Result<()>;
    fn enable_bracketed_paste(&mut self) -> io::Result<()>;
    fn disable_bracketed_paste(&mut self) -> io::Result<()>;
    fn hide_cursor(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
}

/// Production control plane backed by crossterm (any writer; stdout in
/// production, `Vec<u8>` in tests).
#[derive(Debug)]
pub struct CrosstermControl<W: Write + fmt::Debug> {
    writer: W,
}

impl<W: Write + fmt::Debug> CrosstermControl<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Borrow the writer (e.g. for additional escape sequences).
    pub fn writer(&mut self) -> &mut W {
        &mut self.writer
    }
}

impl<W: Write + fmt::Debug> TerminalControl for CrosstermControl<W> {
    fn enable_raw(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }
    fn disable_raw(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }
    fn enter_alt_screen(&mut self) -> io::Result<()> {
        self.writer.execute(EnterAlternateScreen)?;
        Ok(())
    }
    fn leave_alt_screen(&mut self) -> io::Result<()> {
        self.writer.execute(LeaveAlternateScreen)?;
        Ok(())
    }
    fn enable_bracketed_paste(&mut self) -> io::Result<()> {
        execute!(self.writer, crossterm::event::EnableBracketedPaste)
    }
    fn disable_bracketed_paste(&mut self) -> io::Result<()> {
        execute!(self.writer, crossterm::event::DisableBracketedPaste)
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.writer.execute(crossterm::cursor::Hide)?;
        Ok(())
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.writer.execute(crossterm::cursor::Show)?;
        Ok(())
    }
}

/// Recording control plane for unit tests: every operation is appended to
/// `ops` and always succeeds.
#[derive(Debug, Default)]
pub struct FakeControl {
    pub ops: Vec<&'static str>,
}

impl TerminalControl for FakeControl {
    fn enable_raw(&mut self) -> io::Result<()> {
        self.ops.push("enable_raw");
        Ok(())
    }
    fn disable_raw(&mut self) -> io::Result<()> {
        self.ops.push("disable_raw");
        Ok(())
    }
    fn enter_alt_screen(&mut self) -> io::Result<()> {
        self.ops.push("enter_alt_screen");
        Ok(())
    }
    fn leave_alt_screen(&mut self) -> io::Result<()> {
        self.ops.push("leave_alt_screen");
        Ok(())
    }
    fn enable_bracketed_paste(&mut self) -> io::Result<()> {
        self.ops.push("enable_bracketed_paste");
        Ok(())
    }
    fn disable_bracketed_paste(&mut self) -> io::Result<()> {
        self.ops.push("disable_bracketed_paste");
        Ok(())
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.ops.push("hide_cursor");
        Ok(())
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.ops.push("show_cursor");
        Ok(())
    }
}

/// RAII guard applying / restoring [`TerminalModes`] through an injectable
/// [`TerminalControl`].
///
/// Only the delta between the tracked state and the target is ever applied,
/// so repeated `enter`/`restore` calls are idempotent. Restoration runs in
/// the reverse order of entering (cursor → alt screen → paste → raw), and
/// `Drop` restores to [`TerminalModes::OFF`] as the last line of defense.
#[derive(Debug)]
pub struct TerminalGuard<C: TerminalControl> {
    control: C,
    modes: TerminalModes,
}

impl<C: TerminalControl> TerminalGuard<C> {
    /// Guard starting from the all-off baseline.
    pub fn new(control: C) -> Self {
        Self {
            control,
            modes: TerminalModes::OFF,
        }
    }

    /// Current tracked modes (what `restore` would turn off).
    pub fn modes(&self) -> TerminalModes {
        self.modes
    }

    /// Access the underlying control plane (e.g. to queue draws).
    pub fn control(&mut self) -> &mut C {
        &mut self.control
    }

    /// Apply `target` by flipping only the changed switches.
    pub fn enter(&mut self, target: TerminalModes) -> CliResult<()> {
        if target.cursor_hidden && !self.modes.cursor_hidden {
            self.control.hide_cursor()?;
        }
        if target.alt_screen && !self.modes.alt_screen {
            self.control.enter_alt_screen()?;
        }
        if target.bracketed_paste && !self.modes.bracketed_paste {
            self.control.enable_bracketed_paste()?;
        }
        if target.raw && !self.modes.raw {
            self.control.enable_raw()?;
        }
        self.modes = target;
        Ok(())
    }

    /// Restore to the all-off baseline. Idempotent.
    pub fn restore(&mut self) -> CliResult<()> {
        self.restore_to(TerminalModes::OFF)
    }

    /// Restore towards `target` by flipping only the switches that differ,
    /// in reverse enter order.
    pub fn restore_to(&mut self, target: TerminalModes) -> CliResult<()> {
        if self.modes.raw && !target.raw {
            self.control.disable_raw()?;
        }
        if self.modes.bracketed_paste && !target.bracketed_paste {
            self.control.disable_bracketed_paste()?;
        }
        if self.modes.alt_screen && !target.alt_screen {
            self.control.leave_alt_screen()?;
        }
        if self.modes.cursor_hidden && !target.cursor_hidden {
            self.control.show_cursor()?;
        }
        self.modes = target;
        Ok(())
    }

    /// Pause every special mode, optionally lift stderr suppression, run
    /// `op` (an external program with inherited stdio), then re-enter the
    /// recorded modes and re-apply stderr suppression.
    ///
    /// The caller owns the renderer and must schedule a full redraw after
    /// this returns; the guard only restores the terminal *modes*.
    pub fn with_restored<R>(
        &mut self,
        mut stderr: Option<&mut TerminalStderrGuard>,
        op: impl FnOnce() -> R,
    ) -> CliResult<R> {
        let saved = self.modes;
        self.restore()?;
        if let Some(guard) = stderr.as_mut() {
            guard.restore()?;
        }
        let result = op();
        if let Some(guard) = stderr {
            guard.re_suppress()?;
        }
        self.enter(saved)?;
        Ok(result)
    }
}

impl<C: TerminalControl> Drop for TerminalGuard<C> {
    fn drop(&mut self) {
        // Best effort; errors on a dying terminal are unreportable anyway.
        let _ = self.restore();
    }
}

/// Install a process-wide panic hook that restores the *real* terminal
/// (disable paste / show cursor / reset colors / leave alt screen / disable
/// raw mode) before delegating to the previous hook. Idempotent: a second
/// call is a no-op.
pub fn install_panic_hook() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            crossterm::event::DisableBracketedPaste,
            crossterm::cursor::Show,
            crossterm::style::ResetColor,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_then_restore_emits_reversed_sequence() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw"
            ]
        );
        guard.restore().unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
        assert_eq!(guard.modes(), TerminalModes::OFF);
    }

    #[test]
    fn repeated_enter_of_same_modes_is_a_no_op() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::MINI).unwrap();
        let len = guard.control().ops.len();
        guard.enter(TerminalModes::MINI).unwrap();
        assert_eq!(guard.control().ops.len(), len);
    }

    #[test]
    fn enter_delta_only_flips_changed_switches() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::MINI).unwrap();
        // raw + paste stay on; only the cursor becomes hidden.
        guard
            .enter(TerminalModes {
                cursor_hidden: true,
                ..TerminalModes::MINI
            })
            .unwrap();
        assert_eq!(
            guard.control().ops,
            vec!["enable_bracketed_paste", "enable_raw", "hide_cursor"]
        );
    }

    #[test]
    fn double_enter_double_exit_stays_consistent() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        for _ in 0..2 {
            guard.enter(TerminalModes::TUI).unwrap();
            guard.restore().unwrap();
        }
        // Two symmetric enter/restore cycles with no residue in between.
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
        assert_eq!(guard.modes(), TerminalModes::OFF);
    }

    #[test]
    fn restore_is_idempotent() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        guard.restore().unwrap();
        let len = guard.control().ops.len();
        guard.restore().unwrap();
        guard.restore().unwrap();
        assert_eq!(guard.control().ops.len(), len);
    }

    #[test]
    fn with_restored_runs_op_between_restore_and_reenter() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        let ops_before = guard.control().ops.len();

        let answer = guard
            .with_restored(None, || {
                // Inside the window the tracked modes are all off; the
                // restore cycle is the last thing that ran.
                42
            })
            .unwrap();

        assert_eq!(answer, 42);
        assert_eq!(guard.modes(), TerminalModes::TUI);
        let window = &guard.control().ops[ops_before..];
        assert_eq!(
            window,
            vec![
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
            ]
        );
    }

    #[test]
    fn drop_restores_everything() {
        use std::cell::RefCell;
        use std::rc::Rc;

        // Shared recorder so the ops survive the guard drop.
        #[derive(Debug, Default)]
        struct Shared {
            ops: Rc<RefCell<Vec<&'static str>>>,
        }
        impl TerminalControl for Shared {
            fn enable_raw(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_raw");
                Ok(())
            }
            fn disable_raw(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_raw");
                Ok(())
            }
            fn enter_alt_screen(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enter_alt_screen");
                Ok(())
            }
            fn leave_alt_screen(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("leave_alt_screen");
                Ok(())
            }
            fn enable_bracketed_paste(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_bracketed_paste");
                Ok(())
            }
            fn disable_bracketed_paste(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_bracketed_paste");
                Ok(())
            }
            fn hide_cursor(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("hide_cursor");
                Ok(())
            }
            fn show_cursor(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("show_cursor");
                Ok(())
            }
        }

        let ops = Rc::new(RefCell::new(Vec::new()));
        {
            let mut guard = TerminalGuard::new(Shared {
                ops: Rc::clone(&ops),
            });
            guard.enter(TerminalModes::TUI).unwrap();
            assert_eq!(ops.borrow().len(), 4);
        } // drop → restore cycle
        assert_eq!(
            *ops.borrow(),
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
    }
}

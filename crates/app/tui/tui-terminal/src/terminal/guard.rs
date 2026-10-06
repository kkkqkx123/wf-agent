//! RAII guard applying / restoring [`TerminalModes`] through an injectable
//! [`TerminalControl`].

use wf_cli_shared::CliResult;

use crate::stderr::TerminalStderrGuard;

use super::control::TerminalControl;
use super::modes::TerminalModes;

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
        if target.focus_change && !self.modes.focus_change {
            self.control.enable_focus_change()?;
        }
        if target.mouse_capture && !self.modes.mouse_capture {
            self.control.enable_mouse_capture()?;
        }
        if target.alternate_scroll && !self.modes.alternate_scroll {
            self.control.enable_alternate_scroll()?;
        }
        if target.raw && !self.modes.raw {
            self.control.enable_raw()?;
        }
        if target.keyboard_enhancement && !self.modes.keyboard_enhancement {
            self.control.push_keyboard_enhancement()?;
        }
        self.modes = target;
        Ok(())
    }

    /// Restore to the all-off baseline. Idempotent. Keyboard enhancement is
    /// popped first, then the remaining switches unwind in reverse enter
    /// order.
    pub fn restore(&mut self) -> CliResult<()> {
        self.restore_to(TerminalModes::OFF)
    }

    /// Restore towards `target` by flipping only the switches that differ,
    /// in reverse enter order.
    pub fn restore_to(&mut self, target: TerminalModes) -> CliResult<()> {
        if self.modes.keyboard_enhancement && !target.keyboard_enhancement {
            self.control.pop_keyboard_enhancement()?;
        }
        if self.modes.raw && !target.raw {
            self.control.disable_raw()?;
        }
        if self.modes.alternate_scroll && !target.alternate_scroll {
            self.control.disable_alternate_scroll()?;
        }
        if self.modes.mouse_capture && !target.mouse_capture {
            self.control.disable_mouse_capture()?;
        }
        if self.modes.focus_change && !target.focus_change {
            self.control.disable_focus_change()?;
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

    /// Reassert the tracked modes mid-run (focus regained, suspend resumed,
    /// external editor returned): bracketed paste always on, focus / mouse /
    /// alternate scroll per the tracked modes, keyboard enhancement
    /// rewritten with the set form so the push/pop stack stays balanced.
    /// Never disables anything; the tracked modes are unchanged.
    pub fn reassert(&mut self) -> CliResult<()> {
        let modes = self.modes;
        self.control.enable_bracketed_paste()?;
        if modes.focus_change {
            self.control.enable_focus_change()?;
        }
        if modes.mouse_capture {
            self.control.enable_mouse_capture()?;
        }
        if modes.alternate_scroll {
            self.control.enable_alternate_scroll()?;
        }
        if modes.keyboard_enhancement {
            self.control.set_keyboard_enhancement()?;
        }
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

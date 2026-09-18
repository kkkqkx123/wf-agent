//! Terminal interaction facilities shared by the TUI inline and full-screen
//! forms (both inside this crate, unrelated to the `wf-mini` binary).
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
use crossterm::{execute, ExecutableCommand, QueueableCommand};

use wf_cli_shared::CliResult;

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
    /// Focus in/out reporting (`CSI ? 1004 h/l`).
    pub focus_change: bool,
    /// Mouse button/drag/wheel capture with SGR extended coordinates.
    pub mouse_capture: bool,
    /// Alternate scroll (`CSI ? 1007 h/l`): the wheel arrives as up/down
    /// keys without capturing the mouse, so native selection keeps working.
    pub alternate_scroll: bool,
    /// Kitty keyboard enhancement (push on enter, pop on restore).
    pub keyboard_enhancement: bool,
}

impl TerminalModes {
    /// All switches off (the restored baseline).
    pub const OFF: Self = Self {
        raw: false,
        alt_screen: false,
        bracketed_paste: false,
        cursor_hidden: false,
        focus_change: false,
        mouse_capture: false,
        alternate_scroll: false,
        keyboard_enhancement: false,
    };

    /// Typical TUI inline session: inline viewport, no alt screen.
    /// Input capabilities stay off so the byte behavior matches the legacy
    /// baseline; callers opt in via [`Self::with_input_modes`].
    pub const MINI: Self = Self {
        raw: true,
        alt_screen: false,
        bracketed_paste: true,
        cursor_hidden: false,
        focus_change: false,
        mouse_capture: false,
        alternate_scroll: false,
        keyboard_enhancement: false,
    };

    /// Typical full TUI session: alt screen + hidden cursor.
    /// Input capabilities stay off so the byte behavior matches the legacy
    /// baseline; callers opt in via [`Self::with_input_modes`].
    pub const TUI: Self = Self {
        raw: true,
        alt_screen: true,
        bracketed_paste: true,
        cursor_hidden: true,
        focus_change: false,
        mouse_capture: false,
        alternate_scroll: false,
        keyboard_enhancement: false,
    };

    /// Layer the input-capability switches on top of a base mode.
    /// The policy object is the only caller: modes never hardcode sequences.
    pub const fn with_input_modes(
        mut self,
        focus_change: bool,
        mouse_capture: bool,
        alternate_scroll: bool,
        keyboard_enhancement: bool,
    ) -> Self {
        self.focus_change = focus_change;
        self.mouse_capture = mouse_capture;
        self.alternate_scroll = alternate_scroll;
        self.keyboard_enhancement = keyboard_enhancement;
        self
    }
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
    fn enable_focus_change(&mut self) -> io::Result<()>;
    fn disable_focus_change(&mut self) -> io::Result<()>;
    fn enable_mouse_capture(&mut self) -> io::Result<()>;
    fn disable_mouse_capture(&mut self) -> io::Result<()>;
    fn enable_alternate_scroll(&mut self) -> io::Result<()>;
    fn disable_alternate_scroll(&mut self) -> io::Result<()>;
    /// Push the enhancement flags onto the terminal stack (enter path).
    fn push_keyboard_enhancement(&mut self) -> io::Result<()>;
    /// Pop one enhancement level (restore path).
    fn pop_keyboard_enhancement(&mut self) -> io::Result<()>;
    /// Rewrite the enhancement flags without touching the stack
    /// (mid-run reassert path, keeps push/pop balanced).
    fn set_keyboard_enhancement(&mut self) -> io::Result<()>;
    /// Strong reset for exit paths: pop, then clear any leaked level so the
    /// parent shell never inherits enhanced reporting.
    fn reset_keyboard_enhancement(&mut self) -> io::Result<()>;
}

/// Kitty keyboard enhancement flags used by this crate.
///
/// Deliberately excludes `REPORT_ALL_KEYS_AS_ESCAPE_CODES`: some terminals
/// report printable keys as base key plus modifiers, and crossterm cannot
/// map those back to layout-specific shifted symbols on non-US layouts.
pub fn keyboard_enhancement_flags() -> crossterm::event::KeyboardEnhancementFlags {
    use crossterm::event::KeyboardEnhancementFlags;
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
}

/// Environment master switch for keyboard enhancement. Defaults to enabled;
/// an explicit truthy value forces it off, an explicit falsy value forces
/// it on.
pub const DISABLE_KEYBOARD_ENHANCEMENT_ENV: &str = "WF_TUI_DISABLE_KEYBOARD_ENHANCEMENT";

/// True when the environment master switch disables keyboard enhancement.
pub fn keyboard_enhancement_env_disabled() -> bool {
    parse_bool_env(
        std::env::var(DISABLE_KEYBOARD_ENHANCEMENT_ENV)
            .ok()
            .as_deref(),
    )
    .unwrap_or(false)
}

fn parse_bool_env(value: Option<&str>) -> Option<bool> {
    match value.map(str::trim) {
        Some("1") => Some(true),
        Some(value) if value.eq_ignore_ascii_case("true") => Some(true),
        Some(value) if value.eq_ignore_ascii_case("yes") => Some(true),
        Some("0") => Some(false),
        Some(value) if value.eq_ignore_ascii_case("false") => Some(false),
        Some(value) if value.eq_ignore_ascii_case("no") => Some(false),
        _ => None,
    }
}

/// Set-form keyboard enhancement (`CSI = flags u`): rewrites the flags
/// without pushing the stack, for mid-run reasserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SetKeyboardEnhancementFlags;

impl crossterm::Command for SetKeyboardEnhancementFlags {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        write!(f, "\x1b[={}u", keyboard_enhancement_flags().bits())
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "keyboard enhancement not implemented for the legacy Windows API",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false
    }
}

/// Strong keyboard reset (`CSI < u`): clears any leaked stack level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResetKeyboardEnhancementFlags;

impl crossterm::Command for ResetKeyboardEnhancementFlags {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1b[<u")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "keyboard enhancement not implemented for the legacy Windows API",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false
    }
}

/// Alternate scroll on (`CSI ? 1007 h`): the wheel arrives as up/down keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnableAlternateScroll;

impl crossterm::Command for EnableAlternateScroll {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1b[?1007h")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        Err(io::Error::other(
            "alternate scroll requires ANSI sequences, not the legacy Windows API",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

/// Alternate scroll off (`CSI ? 1007 l`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisableAlternateScroll;

impl crossterm::Command for DisableAlternateScroll {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1b[?1007l")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        Err(io::Error::other(
            "alternate scroll requires ANSI sequences, not the legacy Windows API",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

/// Byte sequences emitted when entering `modes` (raw mode excluded: it is
/// termios state, not output bytes). Single choke point for the enter path
/// so tests can assert the exact bytes for each switch combination.
pub fn write_enter_sequences(writer: &mut impl Write, modes: TerminalModes) -> io::Result<()> {
    use crossterm::event::PushKeyboardEnhancementFlags;
    use crossterm::event::{EnableBracketedPaste, EnableFocusChange, EnableMouseCapture};
    if modes.cursor_hidden {
        writer.queue(crossterm::cursor::Hide)?;
    }
    if modes.alt_screen {
        writer.queue(EnterAlternateScreen)?;
    }
    if modes.bracketed_paste {
        writer.queue(EnableBracketedPaste)?;
    }
    if modes.focus_change {
        writer.queue(EnableFocusChange)?;
    }
    if modes.mouse_capture {
        writer.queue(EnableMouseCapture)?;
    }
    if modes.alternate_scroll {
        writer.queue(EnableAlternateScroll)?;
    }
    if modes.keyboard_enhancement {
        writer.queue(PushKeyboardEnhancementFlags(keyboard_enhancement_flags()))?;
    }
    writer.flush()
}

/// Byte sequences emitted when restoring from `modes` towards all-off, in
/// reverse enter order (raw mode excluded).
pub fn write_restore_sequences(writer: &mut impl Write, modes: TerminalModes) -> io::Result<()> {
    use crossterm::event::PopKeyboardEnhancementFlags;
    use crossterm::event::{DisableBracketedPaste, DisableFocusChange, DisableMouseCapture};
    if modes.keyboard_enhancement {
        writer.queue(PopKeyboardEnhancementFlags)?;
    }
    if modes.alternate_scroll {
        writer.queue(DisableAlternateScroll)?;
    }
    if modes.mouse_capture {
        writer.queue(DisableMouseCapture)?;
    }
    if modes.focus_change {
        writer.queue(DisableFocusChange)?;
    }
    if modes.bracketed_paste {
        writer.queue(DisableBracketedPaste)?;
    }
    if modes.alt_screen {
        writer.queue(LeaveAlternateScreen)?;
    }
    if modes.cursor_hidden {
        writer.queue(crossterm::cursor::Show)?;
    }
    writer.flush()
}

/// Byte sequences for a mid-run reassert (focus regained, suspend resumed,
/// external editor returned): bracketed paste always on, focus / mouse /
/// alternate scroll per the modes, keyboard enhancement rewritten with the
/// set form so the push/pop stack stays balanced. Never disables anything.
pub fn write_reassert_sequences(writer: &mut impl Write, modes: TerminalModes) -> io::Result<()> {
    use crossterm::event::{EnableBracketedPaste, EnableFocusChange, EnableMouseCapture};
    writer.queue(EnableBracketedPaste)?;
    if modes.focus_change {
        writer.queue(EnableFocusChange)?;
    }
    if modes.mouse_capture {
        writer.queue(EnableMouseCapture)?;
    }
    if modes.alternate_scroll {
        writer.queue(EnableAlternateScroll)?;
    }
    if modes.keyboard_enhancement {
        writer.queue(SetKeyboardEnhancementFlags)?;
    }
    writer.flush()
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
    fn enable_focus_change(&mut self) -> io::Result<()> {
        execute!(self.writer, crossterm::event::EnableFocusChange)
    }
    fn disable_focus_change(&mut self) -> io::Result<()> {
        execute!(self.writer, crossterm::event::DisableFocusChange)
    }
    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(self.writer, crossterm::event::EnableMouseCapture)
    }
    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(self.writer, crossterm::event::DisableMouseCapture)
    }
    fn enable_alternate_scroll(&mut self) -> io::Result<()> {
        execute!(self.writer, EnableAlternateScroll)
    }
    fn disable_alternate_scroll(&mut self) -> io::Result<()> {
        execute!(self.writer, DisableAlternateScroll)
    }
    fn push_keyboard_enhancement(&mut self) -> io::Result<()> {
        execute!(
            self.writer,
            crossterm::event::PushKeyboardEnhancementFlags(keyboard_enhancement_flags())
        )
    }
    fn pop_keyboard_enhancement(&mut self) -> io::Result<()> {
        execute!(self.writer, crossterm::event::PopKeyboardEnhancementFlags)
    }
    fn set_keyboard_enhancement(&mut self) -> io::Result<()> {
        execute!(self.writer, SetKeyboardEnhancementFlags)
    }
    fn reset_keyboard_enhancement(&mut self) -> io::Result<()> {
        execute!(
            self.writer,
            crossterm::event::PopKeyboardEnhancementFlags,
            ResetKeyboardEnhancementFlags
        )
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
    fn enable_focus_change(&mut self) -> io::Result<()> {
        self.ops.push("enable_focus_change");
        Ok(())
    }
    fn disable_focus_change(&mut self) -> io::Result<()> {
        self.ops.push("disable_focus_change");
        Ok(())
    }
    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        self.ops.push("enable_mouse_capture");
        Ok(())
    }
    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        self.ops.push("disable_mouse_capture");
        Ok(())
    }
    fn enable_alternate_scroll(&mut self) -> io::Result<()> {
        self.ops.push("enable_alternate_scroll");
        Ok(())
    }
    fn disable_alternate_scroll(&mut self) -> io::Result<()> {
        self.ops.push("disable_alternate_scroll");
        Ok(())
    }
    fn push_keyboard_enhancement(&mut self) -> io::Result<()> {
        self.ops.push("push_keyboard_enhancement");
        Ok(())
    }
    fn pop_keyboard_enhancement(&mut self) -> io::Result<()> {
        self.ops.push("pop_keyboard_enhancement");
        Ok(())
    }
    fn set_keyboard_enhancement(&mut self) -> io::Result<()> {
        self.ops.push("set_keyboard_enhancement");
        Ok(())
    }
    fn reset_keyboard_enhancement(&mut self) -> io::Result<()> {
        self.ops.push("reset_keyboard_enhancement");
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

/// True when a controlling terminal is reachable via `/dev/tty`.
pub fn has_controlling_terminal() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .is_ok()
}

/// Orphan rule: stdin hit EOF and no controlling terminal remains, so no
/// input will ever arrive again and the loop should exit instead of
/// spinning on background tasks.
pub fn client_orphaned_with(stdin_eof: bool, has_terminal: bool) -> bool {
    stdin_eof && !has_terminal
}

/// Live orphan check against the real `/dev/tty`.
pub fn client_orphaned(stdin_eof: bool) -> bool {
    client_orphaned_with(stdin_eof, has_controlling_terminal())
}

/// Install a process-wide panic hook that restores the *real* terminal
/// (disable paste / focus / mouse / alternate scroll, keyboard pop plus
/// strong reset, show cursor, reset colors, leave alt screen, disable raw
/// mode) before delegating to the previous hook. Idempotent: a second call
/// is a no-op.
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
            crossterm::event::DisableFocusChange,
            crossterm::event::DisableMouseCapture,
            DisableAlternateScroll,
            crossterm::event::PopKeyboardEnhancementFlags,
            ResetKeyboardEnhancementFlags,
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
            fn enable_focus_change(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_focus_change");
                Ok(())
            }
            fn disable_focus_change(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_focus_change");
                Ok(())
            }
            fn enable_mouse_capture(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_mouse_capture");
                Ok(())
            }
            fn disable_mouse_capture(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_mouse_capture");
                Ok(())
            }
            fn enable_alternate_scroll(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_alternate_scroll");
                Ok(())
            }
            fn disable_alternate_scroll(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_alternate_scroll");
                Ok(())
            }
            fn push_keyboard_enhancement(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("push_keyboard_enhancement");
                Ok(())
            }
            fn pop_keyboard_enhancement(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("pop_keyboard_enhancement");
                Ok(())
            }
            fn set_keyboard_enhancement(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("set_keyboard_enhancement");
                Ok(())
            }
            fn reset_keyboard_enhancement(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("reset_keyboard_enhancement");
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

    #[test]
    fn input_modes_enter_and_restore_in_reverse_order() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(modes).unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_focus_change",
                "enable_mouse_capture",
                "enable_alternate_scroll",
                "enable_raw",
                "push_keyboard_enhancement",
            ]
        );
        guard.restore().unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_focus_change",
                "enable_mouse_capture",
                "enable_alternate_scroll",
                "enable_raw",
                "push_keyboard_enhancement",
                "pop_keyboard_enhancement",
                "disable_raw",
                "disable_alternate_scroll",
                "disable_mouse_capture",
                "disable_focus_change",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
        assert_eq!(guard.modes(), TerminalModes::OFF);
    }

    #[test]
    fn disabled_input_modes_emit_no_extra_ops() {
        // With every input switch off the guard behaves exactly like the
        // legacy baseline: the rollback line for all three capabilities.
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
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
    }

    #[test]
    fn reassert_reenables_without_touching_the_keyboard_stack() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(modes).unwrap();
        guard.control().ops.clear();
        guard.reassert().unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "enable_bracketed_paste",
                "enable_focus_change",
                "enable_mouse_capture",
                "enable_alternate_scroll",
                "set_keyboard_enhancement",
            ]
        );
        // The tracked modes are unchanged by the reassert.
        assert_eq!(guard.modes(), modes);
    }

    #[test]
    fn reassert_with_all_input_off_only_touches_paste() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        guard.control().ops.clear();
        guard.reassert().unwrap();
        assert_eq!(guard.control().ops, vec!["enable_bracketed_paste"]);
    }

    #[test]
    fn keyboard_flags_cover_three_items_without_full_key_reporting() {
        use crossterm::event::KeyboardEnhancementFlags;
        let flags = keyboard_enhancement_flags();
        assert!(flags.contains(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES));
        assert!(flags.contains(KeyboardEnhancementFlags::REPORT_EVENT_TYPES));
        assert!(flags.contains(KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS));
        assert!(!flags.contains(KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES));
    }

    #[test]
    fn keyboard_env_switch_defaults_on_and_parses_explicit_values() {
        let var = DISABLE_KEYBOARD_ENHANCEMENT_ENV;
        let saved = std::env::var(var).ok();
        let restore = |saved: &Option<String>| {
            if let Some(value) = saved {
                std::env::set_var(var, value);
            } else {
                std::env::remove_var(var);
            }
        };

        std::env::remove_var(var);
        assert!(!keyboard_enhancement_env_disabled());
        for truthy in ["1", "true", "TRUE", "yes"] {
            std::env::set_var(var, truthy);
            assert!(keyboard_enhancement_env_disabled(), "value {truthy}");
        }
        for falsy in ["0", "false", "no"] {
            std::env::set_var(var, falsy);
            assert!(!keyboard_enhancement_env_disabled(), "value {falsy}");
        }
        std::env::set_var(var, "unset-value");
        assert!(!keyboard_enhancement_env_disabled());
        restore(&saved);
    }

    #[test]
    fn enter_bytes_cover_all_switches_when_enabled() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut out = Vec::new();
        write_enter_sequences(&mut out, modes).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("\x1b[?25l\x1b[?1049h"));
        assert!(text.contains("\x1b[?2004h"));
        assert!(text.contains("\x1b[?1004h"));
        assert!(text.contains("\x1b[?1000h"));
        assert!(text.contains("\x1b[?1006h"));
        assert!(text.contains("\x1b[?1007h"));
        assert!(text.contains("\x1b[>7u"));
    }

    #[test]
    fn enter_bytes_match_legacy_baseline_when_input_off() {
        let mut out = Vec::new();
        write_enter_sequences(&mut out, TerminalModes::TUI).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b[?25l\x1b[?1049h\x1b[?2004h"
        );
        let mut restore = Vec::new();
        write_restore_sequences(&mut restore, TerminalModes::TUI).unwrap();
        assert_eq!(
            String::from_utf8(restore).unwrap(),
            "\x1b[?2004l\x1b[?1049l\x1b[?25h"
        );
    }

    #[test]
    fn restore_bytes_unwind_in_reverse_enter_order() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut out = Vec::new();
        write_restore_sequences(&mut out, modes).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            concat!(
                "\x1b[<1u",
                "\x1b[?1007l",
                "\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l",
                "\x1b[?1004l",
                "\x1b[?2004l",
                "\x1b[?1049l",
                "\x1b[?25h",
            )
        );
    }

    #[test]
    fn reassert_bytes_use_set_form_and_never_push() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut out = Vec::new();
        write_reassert_sequences(&mut out, modes).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("\x1b[?2004h"));
        assert!(text.contains("\x1b[?1004h"));
        assert!(text.contains("\x1b[?1000h"));
        assert!(text.contains("\x1b[=7u"));
        assert!(
            !text.contains("\x1b[>"),
            "reassert must not push the keyboard stack"
        );

        let mut off = Vec::new();
        write_reassert_sequences(&mut off, TerminalModes::TUI).unwrap();
        assert_eq!(String::from_utf8(off).unwrap(), "\x1b[?2004h");
    }

    #[test]
    fn orphan_rule_needs_eof_without_controlling_terminal() {
        assert!(client_orphaned_with(true, false));
        assert!(!client_orphaned_with(true, true));
        assert!(!client_orphaned_with(false, false));
        assert!(!client_orphaned_with(false, true));
    }
}

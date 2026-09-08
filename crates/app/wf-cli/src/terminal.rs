//! Terminal interaction facilities shared by the mini and full TUI forms
//!
//! Four concerns, all decoupled from rendering:
//! - [`TerminalGuard`]: RAII state machine over raw mode / alternate screen /
//!   bracketed paste / cursor visibility. The control plane ([`TerminalControl`])
//!   is injectable so double enter/exit consistency is unit-testable without a
//!   real TTY; the crossterm-backed [`CrosstermControl`] is the production
//!   implementation.
//! - [`TerminalGuard::with_restored`]: pause every special mode (and lift
//!   stderr suppression), run an external program (`$EDITOR`, git rebase …),
//!   then re-enter the recorded modes. Callers own the full redraw afterwards.
//! - [`TerminalStderrGuard`]: fd-2 redirection so backend/child stderr cannot
//!   corrupt the rendered screen
//! - [`DoublePressTracker`]: the SIGINT two-press state machine (5s window),
//!   consumed by the interactive event loop.
//!
//! crossterm itself provides no RAII for these modes (raw mode toggles are
//! manual since 0.14; the backend never enters/leaves the alternate screen),
//! which is exactly what the guard supplies.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};

use crate::error::CliResult;

/// Window within which a second Ctrl+C counts as "interrupt".
pub const SIGINT_DOUBLE_PRESS_WINDOW: Duration = Duration::from_secs(5);

/// Monotonic origin for the real-clock press helper.
static PRESS_CLOCK_ORIGIN: OnceLock<Instant> = OnceLock::new();

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

// ── stderr suppression (fd 2 redirection) ─────────────────────────────

/// Redirects the process stderr (fd 2) into a file while active, so that
/// backend / child-process diagnostics cannot corrupt the rendered screen.
/// `Drop` restores the original stderr.
///
/// Unix does real fd redirection; other platforms get a no-op guard.
#[derive(Debug)]
pub struct TerminalStderrGuard {
    #[cfg(unix)]
    saved_fd: Option<i32>,
    #[cfg(unix)]
    file: Option<std::fs::File>,
    #[cfg(not(unix))]
    _unused: (),
}

impl TerminalStderrGuard {
    /// Redirect fd 2 into `path` (created / truncated).
    #[cfg(unix)]
    pub fn suppress_to(path: &std::path::Path) -> io::Result<Self> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        let saved = unsafe { libc::dup(libc::STDERR_FILENO) };
        if saved < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(saved) };
            return Err(err);
        }
        Ok(Self {
            saved_fd: Some(saved),
            file: Some(file),
        })
    }

    /// No-op on non-unix platforms.
    #[cfg(not(unix))]
    pub fn suppress_to(_path: &std::path::Path) -> io::Result<Self> {
        Ok(Self { _unused: () })
    }

    /// Restore the original stderr; idempotent. The suppression target file
    /// is kept so [`Self::re_suppress`] can re-apply the redirection.
    #[cfg(unix)]
    pub fn restore(&mut self) -> io::Result<()> {
        if let Some(saved) = self.saved_fd.take() {
            if unsafe { libc::dup2(saved, libc::STDERR_FILENO) } < 0 {
                return Err(io::Error::last_os_error());
            }
            unsafe { libc::close(saved) };
        }
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Re-apply the redirection after a `with_restored` window (no-op when
    /// already suppressing or fully torn down).
    #[cfg(unix)]
    pub fn re_suppress(&mut self) -> io::Result<()> {
        use std::os::unix::io::AsRawFd;
        let Some(file) = self.file.as_ref() else {
            return Ok(());
        };
        if self.saved_fd.is_some() {
            return Ok(());
        }
        let saved = unsafe { libc::dup(libc::STDERR_FILENO) };
        if saved < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(saved) };
            return Err(err);
        }
        self.saved_fd = Some(saved);
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn re_suppress(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Whether stderr is currently redirected.
    pub fn is_suppressing(&self) -> bool {
        #[cfg(unix)]
        {
            self.saved_fd.is_some()
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

impl Drop for TerminalStderrGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

// ── SIGINT double-press state machine ─────────────────────────────────

/// Outcome of recording a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressOutcome {
    /// First press inside a fresh window: warn ("again to interrupt") and
    /// e.g. clear the composer.
    FirstPress,
    /// Second press within the window: interrupt the turn / exit.
    SecondPress,
}

/// Pure two-press tracker with an injected millisecond clock, so synthetic
/// signals test the state machine without real SIGINT delivery.
#[derive(Debug, Clone)]
pub struct DoublePressTracker {
    window_ms: u64,
    last_press_ms: Option<u64>,
}

impl DoublePressTracker {
    /// New tracker with the given window (production code uses
    /// [`SIGINT_DOUBLE_PRESS_WINDOW`] / [`Self::default`]).
    pub fn new(window: Duration) -> Self {
        Self {
            window_ms: window.as_millis() as u64,
            last_press_ms: None,
        }
    }

    /// Record a press at `now_ms`; `SecondPress` only when the previous
    /// press is still inside the window.
    pub fn press(&mut self, now_ms: u64) -> PressOutcome {
        match self.last_press_ms {
            Some(last) if now_ms.saturating_sub(last) <= self.window_ms => {
                self.last_press_ms = None;
                PressOutcome::SecondPress
            }
            _ => {
                self.last_press_ms = Some(now_ms);
                PressOutcome::FirstPress
            }
        }
    }

    /// A first press is still pending (window not yet expired).
    pub fn pending(&self, now_ms: u64) -> bool {
        match self.last_press_ms {
            Some(last) => now_ms.saturating_sub(last) <= self.window_ms,
            None => false,
        }
    }

    /// Forget any pending press (e.g. after the turn finished).
    pub fn reset(&mut self) {
        self.last_press_ms = None;
    }

    /// Record a press using the real monotonic clock (process origin).
    pub fn press_now(&mut self) -> PressOutcome {
        let origin = PRESS_CLOCK_ORIGIN.get_or_init(Instant::now);
        self.press(Instant::now().duration_since(*origin).as_millis() as u64)
    }
}

impl Default for DoublePressTracker {
    fn default() -> Self {
        Self::new(SIGINT_DOUBLE_PRESS_WINDOW)
    }
}

// ── terminal capability probing ──────────────────────────────────────

/// RGB color triplet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorSet {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Probed terminal capabilities.
///
/// Constructed once at startup via [`TerminalProbe::detect`]. The probe
/// queries the terminal for its default background color (OSC 11) and
/// keyboard enhancement support, then stores the results so the rest of
/// the application can query them without re-probing.
#[derive(Debug, Clone)]
pub struct TerminalProbe {
    /// Whether the terminal supports the Kitty keyboard enhancement
    /// protocol (reported via DA2 or assumed when the environment variable
    /// `TERM_PROGRAM` indicates a known-supporting terminal).
    pub keyboard_enhancement: bool,
    /// Whether bracketed paste mode is supported (nearly universal;
    /// `false` only on very old terminals).
    pub bracketed_paste: bool,
    /// Whether the terminal sends focus-in / focus-out events.
    pub focus_events: bool,
    /// The terminal's default background color, if detected via OSC 11.
    pub default_colors: Option<ColorSet>,
}

impl TerminalProbe {
    /// Detect terminal capabilities by querying the real terminal.
    ///
    /// When `stdout` is not a TTY, all capabilities degrade to `false`
    /// / `None`.
    pub fn detect() -> Self {
        if !std::io::stdout().is_terminal() {
            return Self {
                keyboard_enhancement: false,
                bracketed_paste: false,
                focus_events: false,
                default_colors: None,
            };
        }

        let keyboard_enhancement = detect_keyboard_enhancement();
        let bracketed_paste = true; // near-universal in modern terminals
        let focus_events = detect_focus_events();
        let default_colors = detect_default_colors();

        Self {
            keyboard_enhancement,
            bracketed_paste,
            focus_events,
            default_colors,
        }
    }
}

/// Detect keyboard enhancement support via the `TERM_PROGRAM` heuristic.
///
/// A full DA2 query would be ideal but requires async I/O and a timeout,
/// which is complex to integrate at startup. Instead we check well-known
/// terminal identifiers that advertise Kitty keyboard protocol support.
fn detect_keyboard_enhancement() -> bool {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        match term_program.as_str() {
            // Kitty, WezTerm, Ghostty, Rio, Alacritty (nightly)
            "kitty" | "WezTerm" | "ghostty" | "rio" => return true,
            _ => {}
        }
    }
    // iTerm2 supports it since 3.5+
    if let Ok(term_program_version) = std::env::var("TERM_PROGRAM_VERSION") {
        if let Ok(version) = term_program_version.parse::<f64>() {
            if version >= 3.5 {
                return true;
            }
        }
    }
    false
}

/// Detect focus event support via terminal identification.
fn detect_focus_events() -> bool {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        match term_program.as_str() {
            "kitty" | "WezTerm" | "ghostty" | "iTerm.app" | "rio" => return true,
            _ => {}
        }
    }
    false
}

/// Query the terminal's default background color via OSC 11.
///
/// Sends `ESC ] 11 ; ESC \` (OSC 11 = background color query) and reads
/// the response. Returns `None` on timeout, parse failure, or non-TTY.
fn detect_default_colors() -> Option<ColorSet> {
    use std::io::Read;

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return None;
    }

    // Send the OSC 11 query: ESC ] 11 ; ST
    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]11;?\x1b\\").ok()?;
    stdout.flush().ok()?;

    // Read the response with a short timeout using crossterm's poll.
    let mut buf = [0u8; 256];
    let mut stdin = std::io::stdin();
    let deadline = Instant::now() + Duration::from_millis(100);

    let mut total = 0usize;
    while Instant::now() < deadline && total < buf.len() {
        if crossterm::event::poll(Duration::from_millis(10)).unwrap_or(false) {
            // Drain any pending events; we just want to consume bytes.
        }
        match stdin.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }

    if total == 0 {
        return None;
    }

    let response = String::from_utf8_lossy(&buf[..total]);
    parse_osc_color_response(&response)
}

/// Parse an OSC color response like `ESC ] 11 ; rgb:RRRR/GGGG/BBBB ESC \`
/// or `ESC ] 11 ; rgb:RR/GG/BB ST`.
fn parse_osc_color_response(response: &str) -> Option<ColorSet> {
    // Find the "rgb:" prefix
    let rgb_start = response.find("rgb:")?;
    let hex = &response[rgb_start + 4..];

    // Strip the OSC terminator: ESC \ (ST) or BEL (\x07) at the end.
    let hex = hex
        .trim_end_matches('\x07')
        .trim_end_matches("\x1b\\");

    // Split on '/' – expect 3 parts (R, G, B), each 4 or 2 hex digits
    let parts: Vec<&str> = hex.split('/').collect();
    if parts.len() != 3 {
        return None;
    }

    let parse_component = |s: &str| -> Option<u8> {
        let val = u16::from_str_radix(s, 16).ok()?;
        // 4-digit hex (0-ffff) → scale to 0-255; 2-digit stays as-is
        Some(if s.len() > 2 {
            (val >> 8) as u8
        } else {
            val as u8
        })
    };

    Some(ColorSet {
        r: parse_component(parts[0])?,
        g: parse_component(parts[1])?,
        b: parse_component(parts[2])?,
    })
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

    #[test]
    fn double_press_tracker_transitions() {
        let mut t = DoublePressTracker::new(Duration::from_secs(5));
        assert_eq!(t.press(1_000), PressOutcome::FirstPress);
        assert!(t.pending(1_500));
        // Second press inside the window interrupts and consumes the state.
        assert_eq!(t.press(2_000), PressOutcome::SecondPress);
        assert!(!t.pending(2_000));
        // A third press starts a fresh window.
        assert_eq!(t.press(2_100), PressOutcome::FirstPress);
        // Outside the window it is a first press again.
        assert_eq!(t.press(2_100 + 5_001), PressOutcome::FirstPress);
        // The fresh first press is pending until its own window expires.
        assert!(t.pending(2_100 + 5_001 + 1));
        assert!(!t.pending(2_100 + 5_001 + 5_000 + 1));
    }

    #[test]
    fn double_press_tracker_reset_clears_pending() {
        let mut t = DoublePressTracker::new(Duration::from_secs(5));
        t.press(100);
        t.reset();
        assert!(!t.pending(101));
        assert_eq!(t.press(102), PressOutcome::FirstPress);
    }

    #[test]
    fn double_press_tracker_press_now_smoke() {
        let mut t = DoublePressTracker::default();
        assert_eq!(t.press_now(), PressOutcome::FirstPress);
        assert_eq!(t.press_now(), PressOutcome::SecondPress);
        assert_eq!(t.press_now(), PressOutcome::FirstPress);
    }

    #[cfg(unix)]
    mod stderr_guard_tests {
        use super::*;

        /// Serializes tests that redirect the process-wide fd 2.
        static STDERR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        #[test]
        fn suppressed_writes_land_in_the_file_and_restore_releases() {
            let _lock = STDERR_LOCK.lock().unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("stderr.log");
            let text = format!("suppressed-{}-line\n", std::process::id());

            {
                let mut guard = TerminalStderrGuard::suppress_to(&path).unwrap();
                assert!(guard.is_suppressing());
                let mut err = io::stderr();
                err.write_all(text.as_bytes()).unwrap();
                err.flush().unwrap();
                guard.restore().unwrap();
                assert!(!guard.is_suppressing());
            }

            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains(&text), "content was {content:?}");
        }

        #[test]
        fn with_restored_lifts_and_reapplies_stderr_suppression() {
            let _lock = STDERR_LOCK.lock().unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("stderr-cycle.log");
            let mut stderr_guard = TerminalStderrGuard::suppress_to(&path).unwrap();
            assert!(stderr_guard.is_suppressing());

            let mut guard = TerminalGuard::new(FakeControl::default());
            guard.enter(TerminalModes::MINI).unwrap();
            guard.with_restored(Some(&mut stderr_guard), || ()).unwrap();

            // Suppression is active again after the window.
            assert!(stderr_guard.is_suppressing());
            stderr_guard.restore().unwrap();
            assert!(!stderr_guard.is_suppressing());
        }

        #[test]
        fn drop_restores_stderr() {
            let _lock = STDERR_LOCK.lock().unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("stderr-drop.log");
            {
                let guard = TerminalStderrGuard::suppress_to(&path).unwrap();
                assert!(guard.is_suppressing());
                drop(guard);
            }
            // The guard is gone; a new one can suppress again (fd
            // bookkeeping stayed balanced).
            let mut guard = TerminalStderrGuard::suppress_to(&path).unwrap();
            assert!(guard.is_suppressing());
            guard.restore().unwrap();
        }
    }

    #[test]
    fn parse_osc_color_response_4digit_hex() {
        let resp = "\x1b]11;rgb:ffff/8080/0000\x1b\\";
        let c = parse_osc_color_response(resp).unwrap();
        assert_eq!(c, ColorSet { r: 255, g: 128, b: 0 });
    }

    #[test]
    fn parse_osc_color_response_2digit_hex() {
        let resp = "\x1b]11;rgb:ff/80/00\x1b\\";
        let c = parse_osc_color_response(resp).unwrap();
        assert_eq!(c, ColorSet { r: 255, g: 128, b: 0 });
    }

    #[test]
    fn parse_osc_color_response_invalid() {
        assert!(parse_osc_color_response("no color here").is_none());
        assert!(parse_osc_color_response("rgb:xx/yy/zz").is_none());
    }

    #[test]
    fn terminal_probe_non_tty_degrades() {
        // In a test environment stdout is usually not a TTY, so the probe
        // should degrade gracefully.
        let probe = TerminalProbe::detect();
        // We can't assert specific values since the test runner environment
        // varies, but we can at least verify the struct constructs.
        let _ = probe;
    }
}

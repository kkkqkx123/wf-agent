//! Clipboard backend with local-tool priority and OSC 52 fallback.
//!
//! The emission order is deliberate: a local clipboard tool either succeeds
//! or fails loudly, while writing an OSC 52 sequence to stdout always
//! "succeeds" whether or not any terminal acts on it. Preferring OSC 52
//! would silently drop the copy on the many terminals that ignore it, with
//! no failure to fall back from.
//!
//! When both X11 and Wayland displays are reachable the X11 tools are tried
//! first: owning a Wayland selection needs an input serial (hence keyboard
//! focus and a mapped surface) unless the compositor implements
//! `wlr-data-control`, so the Wayland path flashes a visible entry on
//! desktops without it while the X11 bridge propagates invisibly.
//!
//! The choice pins via [`set_clipboard_mode`] or the
//! `TUI_PANEL_SELECT_CLIPBOARD` environment variable (`auto`, `x11`,
//! `wayland`, `osc52`, `none`). Test suites must disable the backend: any
//! exercised copy path would otherwise overwrite the developer clipboard
//! and leak helper processes.

use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};

/// Pin the clipboard backend, overriding environment detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardMode {
    /// Try local tools first, fall back to OSC 52.
    Auto,
    /// Only X11 tools (`xclip`, `xsel`).
    X11,
    /// Only Wayland tools (`wl-copy`).
    Wayland,
    /// Only the OSC 52 escape sequence.
    Osc52,
    /// Never write anywhere.
    None,
}

impl ClipboardMode {
    /// Parse an environment or configuration value. Returns `None` for
    /// unknown input so callers fall back to [`ClipboardMode::Auto`] instead
    /// of silently disabling the copy on a typo.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(ClipboardMode::Auto),
            "x11" | "xclip" | "xsel" => Some(ClipboardMode::X11),
            "wayland" | "wl-copy" | "wl" => Some(ClipboardMode::Wayland),
            "osc52" | "osc" | "terminal" => Some(ClipboardMode::Osc52),
            "none" | "off" | "disabled" | "0" => Some(ClipboardMode::None),
            _ => None,
        }
    }
}

/// Environment variable pinning the backend.
pub const CLIPBOARD_ENV: &str = "TUI_PANEL_SELECT_CLIPBOARD";

/// Process-wide override: 0 follows the environment, 1..=5 pin a mode.
static MODE_OVERRIDE: AtomicU8 = AtomicU8::new(0);

fn mode_to_code(mode: ClipboardMode) -> u8 {
    match mode {
        ClipboardMode::Auto => 1,
        ClipboardMode::X11 => 2,
        ClipboardMode::Wayland => 3,
        ClipboardMode::Osc52 => 4,
        ClipboardMode::None => 5,
    }
}

fn code_to_mode(code: u8) -> Option<ClipboardMode> {
    match code {
        1 => Some(ClipboardMode::Auto),
        2 => Some(ClipboardMode::X11),
        3 => Some(ClipboardMode::Wayland),
        4 => Some(ClipboardMode::Osc52),
        5 => Some(ClipboardMode::None),
        _ => None,
    }
}

/// Pin the clipboard backend for this process.
pub fn set_clipboard_mode(mode: ClipboardMode) {
    MODE_OVERRIDE.store(mode_to_code(mode), Ordering::SeqCst);
}

/// Resolve the active backend: process override first, then the
/// environment, defaulting to [`ClipboardMode::Auto`].
pub fn clipboard_mode() -> ClipboardMode {
    if let Some(mode) = code_to_mode(MODE_OVERRIDE.load(Ordering::SeqCst)) {
        return mode;
    }
    std::env::var(CLIPBOARD_ENV)
        .ok()
        .and_then(|value| ClipboardMode::parse(&value))
        .unwrap_or(ClipboardMode::Auto)
}

/// OSC 52 sequence for `text`. Inside tmux the sequence is wrapped in a
/// pass-through so the multiplexer forwards it to the outer terminal.
pub fn osc52_sequence(text: &str, in_tmux: bool) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let inner = format!("\x1b]52;c;{encoded}\x07");
    if in_tmux {
        let escaped = inner.replace('\x1b', "\x1b\x1b");
        format!("\x1bPtmux;{escaped}\x1b\\")
    } else {
        inner
    }
}

/// Whether stdout is wrapped by tmux.
pub fn in_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Local clipboard tool candidates for the current environment. Each entry
/// is the binary plus its fixed arguments; text travels over stdin.
pub fn clipboard_candidates(
    has_wayland: bool,
    has_x11: bool,
    is_macos: bool,
    mode: ClipboardMode,
) -> Vec<(&'static str, &'static [&'static str])> {
    let want_x11 = has_x11 && matches!(mode, ClipboardMode::Auto | ClipboardMode::X11);
    let want_wayland = has_wayland && matches!(mode, ClipboardMode::Auto | ClipboardMode::Wayland);
    let mut candidates = Vec::new();
    if want_x11 {
        candidates.push(("xclip", &["-selection", "clipboard"][..]));
        candidates.push(("xsel", &["--clipboard", "--input"][..]));
    }
    if want_wayland {
        candidates.push(("wl-copy", &[][..]));
    }
    if mode != ClipboardMode::Auto {
        return candidates;
    }
    if is_macos {
        candidates.push(("pbcopy", &[][..]));
    }
    candidates.push(("clip.exe", &[][..]));
    candidates
}

fn display_reachable() -> (bool, bool, bool) {
    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    let has_x11 = std::env::var("DISPLAY").is_ok();
    (has_wayland, has_x11, cfg!(target_os = "macos"))
}

/// Try the local clipboard tools for `text`. Returns `true` on the first
/// tool that accepts the payload over stdin. Helper processes are reaped on
/// a background thread so forked daemons never linger as zombies.
pub fn try_external_clipboard(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let mode = clipboard_mode();
    if matches!(mode, ClipboardMode::Osc52 | ClipboardMode::None) {
        return false;
    }
    let (has_wayland, has_x11, is_macos) = display_reachable();
    for (binary, args) in clipboard_candidates(has_wayland, has_x11, is_macos, mode) {
        if spawn_clipboard_helper(binary, args, text) {
            return true;
        }
    }
    false
}

fn spawn_clipboard_helper(binary: &str, args: &[&str], text: &str) -> bool {
    use std::process::Stdio;
    let mut command = std::process::Command::new(binary);
    command.args(args);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return false,
    };
    let accepted = child
        .stdin
        .as_mut()
        .and_then(|stdin| stdin.write_all(text.as_bytes()).ok())
        .is_some();
    drop(child.stdin.take());
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parsing_covers_aliases_and_rejects_typos() {
        assert_eq!(ClipboardMode::parse("auto"), Some(ClipboardMode::Auto));
        assert_eq!(ClipboardMode::parse(" XCLIP "), Some(ClipboardMode::X11));
        assert_eq!(
            ClipboardMode::parse("wl-copy"),
            Some(ClipboardMode::Wayland)
        );
        assert_eq!(ClipboardMode::parse("osc52"), Some(ClipboardMode::Osc52));
        assert_eq!(ClipboardMode::parse("none"), Some(ClipboardMode::None));
        assert_eq!(ClipboardMode::parse("0"), Some(ClipboardMode::None));
        assert_eq!(ClipboardMode::parse("sometimes"), None);
        assert_eq!(ClipboardMode::parse(""), None);
    }

    #[test]
    fn x11_tools_come_before_wayland_tools() {
        let candidates = clipboard_candidates(true, true, false, ClipboardMode::Auto);
        let names: Vec<&str> = candidates.iter().map(|(binary, _)| *binary).collect();
        let xclip = names
            .iter()
            .position(|name| *name == "xclip")
            .expect("xclip");
        let wl = names
            .iter()
            .position(|name| *name == "wl-copy")
            .expect("wl-copy");
        assert!(xclip < wl, "x11 first: {names:?}");
    }

    #[test]
    fn pinned_modes_narrow_the_candidates() {
        let x11 = clipboard_candidates(true, true, false, ClipboardMode::X11);
        assert!(x11.iter().all(|(binary, _)| *binary != "wl-copy"));
        assert!(!x11.is_empty());
        let wayland = clipboard_candidates(true, true, false, ClipboardMode::Wayland);
        assert_eq!(wayland.len(), 1);
        assert_eq!(wayland[0].0, "wl-copy");
        assert!(clipboard_candidates(true, true, false, ClipboardMode::Osc52).is_empty());
        assert!(clipboard_candidates(true, true, false, ClipboardMode::None).is_empty());
    }

    #[test]
    fn osc52_sequence_wraps_for_tmux_passthrough() {
        let plain = osc52_sequence("hi", false);
        assert!(plain.starts_with("\x1b]52;c;"), "{plain:?}");
        assert!(plain.ends_with('\x07'), "{plain:?}");
        assert!(plain.contains("aGk="), "{plain:?}");
        let wrapped = osc52_sequence("hi", true);
        assert!(wrapped.starts_with("\x1bPtmux;"), "{wrapped:?}");
        assert!(wrapped.ends_with("\x1b\\"), "{wrapped:?}");
    }

    #[test]
    fn process_override_wins_over_environment() {
        set_clipboard_mode(ClipboardMode::None);
        assert_eq!(clipboard_mode(), ClipboardMode::None);
        assert!(!try_external_clipboard("payload"));
        set_clipboard_mode(ClipboardMode::Auto);
        assert_eq!(clipboard_mode(), ClipboardMode::Auto);
    }
}

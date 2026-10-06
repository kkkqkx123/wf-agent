//! crossterm command structs for keyboard enhancement and alternate scroll,
//! plus the keyboard-enhancement flag set and its environment master switch.

use std::fmt;
#[cfg(windows)]
use std::io;

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
pub(crate) struct SetKeyboardEnhancementFlags;

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
pub(crate) struct ResetKeyboardEnhancementFlags;

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
pub(crate) struct EnableAlternateScroll;

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
pub(crate) struct DisableAlternateScroll;

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

//! Centralized icon and status-symbol definitions for the TUI.

pub const ASSISTANT_BULLET: &str = "●";
pub const TOOL_CALL: &str = "▲";
pub const SUCCESS: &str = "✓";
pub const FAILURE: &str = "✗";
pub const RUNNING: &str = "▶";
pub const PAUSED: &str = "⏸";
pub const READY: &str = "○";
pub const WARNING: &str = "⚠";

pub const STATUS_ASCII: [&str; 8] = ["*", ">", "+", "x", ">", "||", "o", "!"];

pub const LEFT: &str = "←";
pub const RIGHT: &str = "→";
pub const UP: &str = "↑";
pub const DOWN: &str = "↓";
pub const MORE: &str = "…";

pub const EXECUTION: &str = "▣";
pub const USAGE: &str = "▧";

pub const SPINNER_BRAILLE_10: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub const SPINNER_BRAILLE_4: [&str; 4] = ["⣾", "⣽", "⣻", "⢿"];

pub const SPINNER_ASCII_8: [&str; 8] = ["|", "/", "-", "\\", "|", "/", "-", "\\"];

/// Return an ASCII fallback for a known UI glyph, if any.
pub fn ascii_fallback(ch: char) -> Option<&'static str> {
    match ch {
        '●' => Some("*"),
        '▲' => Some(">"),
        '✓' => Some("+"),
        '✗' => Some("x"),
        '▶' => Some(">"),
        '⏸' => Some("||"),
        '○' => Some("o"),
        '⚠' => Some("!"),
        '←' => Some("<"),
        '→' => Some(">"),
        '↑' => Some("^"),
        '↓' => Some("v"),
        '…' => Some("..."),
        '▣' => Some("#"),
        '▧' => Some("%"),
        _ => None,
    }
}

//! Mini-mode splash: the ASCII logo shown when `wf --mini` starts and the
//! one-line goodbye shown on exit.
//!
//! Kept as pure data (a `Vec<String>` of equal-width logo rows) so the mini
//! event loop can push it into the scrollback like any other line — no
//! alternate-screen or animation, so it survives the inline split-footer
//! model and the terminal's own scrollback.

/// The logo rows (already padded to a uniform width).
pub fn splash_lines() -> Vec<String> {
    vec![
        "  ██╗    ██╗ █████╗     ".to_string(),
        "  ██║    ██║██╔══██╗    ".to_string(),
        "  ██║ █╗ ██║███████║    wf-agent · mini".to_string(),
        "  ██║███╗██║██╔══██║    lightweight session".to_string(),
        "   ╚███╔███╔╝██║  ██║    ".to_string(),
        "    ╚══╝╚═╝ ╚═╝  ╚═╝    ".to_string(),
    ]
}

/// A one-line farewell rendered when the mini session exits.
pub fn goodbye_line() -> String {
    "◆ session ended — thanks for using wf-agent".to_string()
}

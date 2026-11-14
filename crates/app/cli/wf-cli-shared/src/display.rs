//! Mini display options shared by the CLI frontends.
//!
//! The lightweight interactive form (`wf-mini`) renders assistant text to
//! stdout and diagnostics to stderr without a fullscreen redraw. These
//! options select how markdown source is presented and which stderr streams
//! are shown. They carry no TUI or markdown-parser dependencies so the
//! shared crate stays lean; parsing and rendering live in `wf-mini`.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// How assistant markdown source is presented on stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Serialize, Deserialize)]
#[clap(rename_all = "lower")]
#[serde(rename_all = "lowercase")]
pub enum MarkdownMode {
    /// Stream sanitized source verbatim (historical behavior).
    #[default]
    Raw,
    /// Strip markdown markup to plain text, no ANSI escapes.
    Plain,
    /// Style markdown with ANSI SGR when color is available, otherwise
    /// degrade to plain text. Never emits raw model escapes.
    Styled,
}

impl MarkdownMode {
    /// Parse a user-supplied value (flag, env, config file).
    pub fn parse_opt(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "raw" => Some(Self::Raw),
            "plain" => Some(Self::Plain),
            "styled" => Some(Self::Styled),
            _ => None,
        }
    }

    /// Short name used in help, banners and `/display`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Plain => "plain",
            Self::Styled => "styled",
        }
    }
}

impl Display for MarkdownMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Parse an on/off value for the stderr toggles.
pub fn parse_toggle(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" | "y" => Some(true),
        "off" | "false" | "0" | "no" | "n" => Some(false),
        _ => None,
    }
}

/// Parse a disabling env flag (`1/true/yes/on` means disabled).
pub fn parse_disable_flag(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

/// Shared ANSI gate: `NO_COLOR` or a `dumb` terminal disables styling on
/// every stream. TTY and `--no-color` checks stay per-stream so stdout and
/// stderr gates remain independent.
pub fn ansi_allowed() -> bool {
    if std::env::var("NO_COLOR")
        .map(|raw| !raw.trim().is_empty())
        .unwrap_or(false)
    {
        return false;
    }
    if std::env::var("TERM")
        .map(|raw| raw.trim().eq_ignore_ascii_case("dumb"))
        .unwrap_or(false)
    {
        return false;
    }
    true
}

/// ANSI styling is emitted only for human TTY output: an explicit
/// `no_color`, a non-TTY stdout, `NO_COLOR` or a `dumb` terminal all force
/// plain output.
pub fn color_enabled(no_color: bool, tty: bool) -> bool {
    if no_color || !tty {
        return false;
    }
    ansi_allowed()
}

/// Stderr diagnostics follow the same gate but on the stderr TTY, kept
/// independent from the stdout `color` flag so disabling stdout styling
/// never silences stderr styling and vice versa.
pub fn stderr_color_enabled(no_color: bool, tty: bool) -> bool {
    if no_color || !tty {
        return false;
    }
    ansi_allowed()
}

/// Read an on/off environment variable (`on/true/1/yes` means on,
/// `off/false/0/no` means off). Anything else, including unset, means
/// no opinion so the caller falls back to flags, files and defaults.
pub fn env_toggle(name: &str) -> Option<bool> {
    std::env::var(name).ok().and_then(|raw| parse_toggle(&raw))
}

/// Read a disabling environment variable (`1/true/yes/on` means disabled).
/// Anything else, including unset, means no opinion.
pub fn env_disable_flag(name: &str) -> Option<bool> {
    std::env::var(name)
        .ok()
        .and_then(|raw| parse_disable_flag(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_mode_parses_case_insensitively() {
        assert_eq!(MarkdownMode::parse_opt("raw"), Some(MarkdownMode::Raw));
        assert_eq!(
            MarkdownMode::parse_opt(" PLAIN "),
            Some(MarkdownMode::Plain)
        );
        assert_eq!(
            MarkdownMode::parse_opt("Styled"),
            Some(MarkdownMode::Styled)
        );
        assert_eq!(MarkdownMode::parse_opt("html"), None);
    }

    #[test]
    fn markdown_mode_default_is_raw() {
        assert_eq!(MarkdownMode::default(), MarkdownMode::Raw);
        assert_eq!(MarkdownMode::Raw.as_str(), "raw");
    }

    #[test]
    fn toggles_parse_on_off_forms() {
        assert_eq!(parse_toggle("on"), Some(true));
        assert_eq!(parse_toggle("OFF"), Some(false));
        assert_eq!(parse_toggle("maybe"), None);
        assert_eq!(parse_disable_flag("1"), Some(true));
        assert_eq!(parse_disable_flag("0"), Some(false));
        assert_eq!(parse_disable_flag("bogus"), None);
    }

    #[test]
    fn color_gates_require_tty_and_clean_environment() {
        assert!(!super::color_enabled(true, true));
        assert!(!super::color_enabled(false, false));
        assert!(!super::stderr_color_enabled(true, true));
        assert!(!super::stderr_color_enabled(false, false));
    }
}

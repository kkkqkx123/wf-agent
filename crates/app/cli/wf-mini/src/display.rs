//! Display configuration for the native session.
//!
//! Resolution order for every knob is CLI flag, environment, config file,
//! built-in default. Defaults preserve the historical behavior: verbatim
//! markdown, all stderr streams shown, 120s approval timeout.

use wf_cli_shared::app_config::{self, MiniConfig};
use wf_cli_shared::args::Cli;
use wf_cli_shared::display::MarkdownMode;

/// Approval prompt timeout bounds (seconds).
pub const MIN_APPROVAL_TIMEOUT_SECS: u64 = 5;
pub const MAX_APPROVAL_TIMEOUT_SECS: u64 = 3600;
pub const DEFAULT_APPROVAL_TIMEOUT_SECS: u64 = 120;

/// Resolved display settings for one session. `markdown` selects the stdout
/// presentation; the `show_*` flags gate stderr streams (failures always
/// show); `color` records whether ANSI styling may be emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayConfig {
    pub markdown: MarkdownMode,
    pub show_reasoning: bool,
    pub show_usage: bool,
    pub show_tools: bool,
    pub color: bool,
    pub approval_timeout_secs: u64,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            markdown: MarkdownMode::Raw,
            show_reasoning: true,
            show_usage: true,
            show_tools: true,
            color: false,
            approval_timeout_secs: DEFAULT_APPROVAL_TIMEOUT_SECS,
        }
    }
}

impl DisplayConfig {
    /// Resolve from CLI flags, environment, config file and TTY state.
    pub fn resolve(cli: &Cli, stdout_tty: bool) -> Self {
        let file = load_file_config();
        let markdown = cli
            .markdown
            .or_else(env_markdown)
            .or_else(|| file.as_ref().and_then(|config| config.markdown))
            .unwrap_or_default();
        let show_reasoning = resolve_show(
            cli.no_reasoning,
            "WF_MINI_NO_REASONING",
            file.as_ref().and_then(|config| config.show_reasoning),
        );
        let show_usage = resolve_show(
            cli.no_usage,
            "WF_MINI_NO_USAGE",
            file.as_ref().and_then(|config| config.show_usage),
        );
        let show_tools = resolve_show(
            cli.quiet_tools,
            "WF_MINI_QUIET_TOOLS",
            file.as_ref().and_then(|config| config.show_tools),
        );
        let approval_timeout_secs = resolve_approval_timeout(file.as_ref());
        let color = color_enabled(cli, stdout_tty);
        Self {
            markdown,
            show_reasoning,
            show_usage,
            show_tools,
            color,
            approval_timeout_secs,
        }
    }

    /// One-line summary for the banner and `/display`.
    pub fn describe(&self) -> String {
        format!(
            "markdown={} reasoning={} usage={} tools={} color={} approval_timeout={}s",
            self.markdown.as_str(),
            on_off(self.show_reasoning),
            on_off(self.show_usage),
            on_off(self.show_tools),
            on_off(self.color),
            self.approval_timeout_secs,
        )
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn resolve_show(cli_hide: bool, env_name: &str, file_show: Option<bool>) -> bool {
    if cli_hide {
        return false;
    }
    if let Some(disabled) = env_disable_flag(env_name) {
        return !disabled;
    }
    file_show.unwrap_or(true)
}

fn env_markdown() -> Option<MarkdownMode> {
    std::env::var("WF_MINI_MARKDOWN")
        .ok()
        .and_then(|raw| MarkdownMode::parse_opt(&raw))
}

fn env_disable_flag(name: &str) -> Option<bool> {
    std::env::var(name)
        .ok()
        .and_then(|raw| match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => Some(true),
            "0" | "false" | "no" | "n" | "off" => Some(false),
            _ => None,
        })
}

fn resolve_approval_timeout(file: Option<&MiniConfig>) -> u64 {
    if let Ok(raw) = std::env::var("WF_MINI_APPROVAL_TIMEOUT") {
        if let Ok(parsed) = raw.trim().parse::<u64>() {
            return parsed.clamp(MIN_APPROVAL_TIMEOUT_SECS, MAX_APPROVAL_TIMEOUT_SECS);
        }
    }
    file.and_then(|config| config.approval_timeout_secs)
        .map(|secs| secs.clamp(MIN_APPROVAL_TIMEOUT_SECS, MAX_APPROVAL_TIMEOUT_SECS))
        .unwrap_or(DEFAULT_APPROVAL_TIMEOUT_SECS)
}

fn load_file_config() -> Option<MiniConfig> {
    let path = app_config::config_file_path()?;
    app_config::ConfigManager::load(&path)
        .ok()
        .map(|manager| manager.config().mini.clone())
}

/// ANSI styling is emitted only for human TTY output: an explicit
/// `--no-color`, a non-TTY stdout, `NO_COLOR` or a `dumb` terminal all force
/// plain output. Styled mode degrades to plain text under these conditions
/// instead of emitting escapes into pipes or files.
pub fn color_enabled(cli: &Cli, stdout_tty: bool) -> bool {
    if cli.no_color || !stdout_tty {
        return false;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_preserve_historical_behavior() {
        let config = DisplayConfig::default();
        assert_eq!(config.markdown, MarkdownMode::Raw);
        assert!(config.show_reasoning);
        assert!(config.show_usage);
        assert!(config.show_tools);
        assert_eq!(config.approval_timeout_secs, 120);
    }

    #[test]
    fn approval_timeout_clamps_to_bounds() {
        assert_eq!(
            3u64.clamp(MIN_APPROVAL_TIMEOUT_SECS, MAX_APPROVAL_TIMEOUT_SECS),
            MIN_APPROVAL_TIMEOUT_SECS
        );
        assert_eq!(
            99999u64.clamp(MIN_APPROVAL_TIMEOUT_SECS, MAX_APPROVAL_TIMEOUT_SECS),
            MAX_APPROVAL_TIMEOUT_SECS
        );
    }
}

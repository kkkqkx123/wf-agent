//! Display configuration for the native session.
//!
//! Resolution order for every knob is CLI flag, environment, config file,
//! built-in default. Defaults preserve the historical behavior: verbatim
//! markdown, all stderr streams shown, 120s approval timeout.

use tui_core::tool_output::{DetailLevel, ToolOutputConfig};
use wf_cli_shared::app_config::{self, MiniConfig};
use wf_cli_shared::args::Cli;
use wf_cli_shared::display::MarkdownMode;

/// Approval prompt timeout bounds (seconds).
pub const MIN_APPROVAL_TIMEOUT_SECS: u64 = 5;
pub const MAX_APPROVAL_TIMEOUT_SECS: u64 = 3600;
pub const DEFAULT_APPROVAL_TIMEOUT_SECS: u64 = 120;

/// Ghost suggestion display limit bounds (characters).
pub const MIN_SUGGESTION_DISPLAY_LIMIT: usize = 10;
pub const MAX_SUGGESTION_DISPLAY_LIMIT: usize = 200;
pub const DEFAULT_SUGGESTION_DISPLAY_LIMIT: usize = 30;

/// Resolved display settings for one session. `markdown` selects the stdout
/// presentation; the `show_*` flags gate stderr streams (failures always
/// show); `color` records whether ANSI styling may be emitted on stdout,
/// `stderr_color` the same for stderr diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayConfig {
    pub markdown: MarkdownMode,
    pub show_reasoning: bool,
    pub show_usage: bool,
    pub show_tools: bool,
    pub color: bool,
    pub stderr_color: bool,
    pub approval_timeout_secs: u64,
    pub suggestion_display_limit: usize,
    pub tool_output: ToolOutputConfig,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            markdown: MarkdownMode::Raw,
            show_reasoning: true,
            show_usage: true,
            show_tools: true,
            color: false,
            stderr_color: false,
            approval_timeout_secs: DEFAULT_APPROVAL_TIMEOUT_SECS,
            suggestion_display_limit: DEFAULT_SUGGESTION_DISPLAY_LIMIT,
            tool_output: ToolOutputConfig::default(),
        }
    }
}

impl DisplayConfig {
    /// Resolve from CLI flags, environment, config file and TTY state.
    pub fn resolve(cli: &Cli, stdout_tty: bool, stderr_tty: bool) -> Self {
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
        let suggestion_display_limit = resolve_suggestion_display_limit(file.as_ref());
        let tool_output = resolve_tool_output(cli);
        let color = color_enabled(cli, stdout_tty);
        let stderr_color = stderr_color_enabled(cli, stderr_tty);
        Self {
            markdown,
            show_reasoning,
            show_usage,
            show_tools,
            color,
            stderr_color,
            approval_timeout_secs,
            suggestion_display_limit,
            tool_output,
        }
    }

    /// Whether success tool lines may render given both masters.
    pub fn effective_show_tools(&self) -> bool {
        self.show_tools && self.tool_output.enabled
    }

    /// One-line summary for the banner and `/display`.
    pub fn describe(&self) -> String {
        format!(
            "markdown={} reasoning={} usage={} tools={} tool_detail={} color={} stderr_color={} approval_timeout={}s suggestion_display_limit={}",
            self.markdown.as_str(),
            on_off(self.show_reasoning),
            on_off(self.show_usage),
            on_off(self.show_tools),
            self.tool_output.level.label(),
            on_off(self.color),
            on_off(self.stderr_color),
            self.approval_timeout_secs,
            self.suggestion_display_limit,
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

fn resolve_suggestion_display_limit(file: Option<&MiniConfig>) -> usize {
    if let Ok(raw) = std::env::var("WF_MINI_SUGGESTION_DISPLAY_LIMIT") {
        if let Ok(parsed) = raw.trim().parse::<usize>() {
            return parsed.clamp(MIN_SUGGESTION_DISPLAY_LIMIT, MAX_SUGGESTION_DISPLAY_LIMIT);
        }
    }
    file.and_then(|config| config.suggestion_display_limit)
        .map(|value| value.clamp(MIN_SUGGESTION_DISPLAY_LIMIT, MAX_SUGGESTION_DISPLAY_LIMIT))
        .unwrap_or(DEFAULT_SUGGESTION_DISPLAY_LIMIT)
}

fn load_file_config() -> Option<MiniConfig> {
    let path = app_config::config_file_path()?;
    app_config::ConfigManager::load(&path)
        .ok()
        .map(|manager| manager.config().mini.clone())
}

/// Parse a tool detail level name from flag, env or slash command.
pub fn parse_tool_detail(raw: &str) -> Option<DetailLevel> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "low" => Some(DetailLevel::Low),
        "medium" => Some(DetailLevel::Medium),
        "high" => Some(DetailLevel::High),
        _ => None,
    }
}

/// Tool output thresholds for one mini session. Preview lengths come from
/// the shared `[tool_output]` section so mini and the full TUI stay on one
/// vocabulary; only the level has a CLI and env override. Unknown level
/// values fall back to the file or default instead of failing the session.
fn resolve_tool_output(cli: &Cli) -> ToolOutputConfig {
    use wf_cli_shared::app_config::ToolDetailLevelName;
    let mut config = ToolOutputConfig::default();
    if let Some(path) = app_config::config_file_path() {
        if let Ok(manager) = app_config::ConfigManager::load(&path) {
            let file = &manager.config().tool_output;
            if let Some(enabled) = file.enabled {
                config.enabled = enabled;
            }
            if let Some(lines) = file.preview_max_lines {
                config.preview_max_lines = lines;
            }
            if let Some(chars) = file.preview_max_chars {
                config.preview_max_chars = chars;
            }
            if let Some(lines) = file.shell_preview_lines {
                config.shell_preview_lines = lines;
            }
            if let Some(lines) = file.error_preview_lines {
                config.error_preview_lines = lines;
            }
            if let Some(bytes) = file.block_max_bytes {
                config.block_max_bytes = bytes;
            }
            if let Some(level) = file.level {
                config.level = match level {
                    ToolDetailLevelName::Low => DetailLevel::Low,
                    ToolDetailLevelName::Medium => DetailLevel::Medium,
                    ToolDetailLevelName::High => DetailLevel::High,
                };
            }
        }
    }
    let env_level = std::env::var("WF_MINI_TOOL_DETAIL")
        .ok()
        .and_then(|raw| parse_tool_detail(&raw));
    let cli_level = cli.tool_detail.as_deref().and_then(parse_tool_detail);
    if let Some(level) = cli_level.or(env_level) {
        config.level = level;
    }
    config
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
/// `--no-color`, a non-TTY stdout, `NO_COLOR` or a `dumb` terminal all force
/// plain output. Styled mode degrades to plain text under these conditions
/// instead of emitting escapes into pipes or files.
pub fn color_enabled(cli: &Cli, stdout_tty: bool) -> bool {
    if cli.no_color || !stdout_tty {
        return false;
    }
    ansi_allowed()
}

/// Stderr diagnostics follow the same gate but on the stderr TTY, kept
/// independent from the stdout `color` flag so disabling stdout styling
/// never silences stderr styling and vice versa.
pub fn stderr_color_enabled(cli: &Cli, stderr_tty: bool) -> bool {
    if cli.no_color || !stderr_tty {
        return false;
    }
    ansi_allowed()
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

//! Minimal user configuration for the CLI/TUI frontends.
//!
//! Only options with real consumers live here (`mouse_capture`,
//! `copy_target`). The file follows XDG conventions:
//! `$XDG_CONFIG_HOME/wf/config.toml` or `$HOME/.config/wf/config.toml`. A
//! missing or unreadable file means defaults; `load` never fails the caller
//! on bad contents.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::display::MarkdownMode;

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Behavioral settings.
    #[serde(default)]
    pub behavior: BehaviorConfig,
    /// Terminal channel settings (clipboard copy target).
    #[serde(default)]
    pub terminal: TerminalConfig,
    /// Mini interactive-form display settings.
    #[serde(default)]
    pub mini: MiniConfig,
    /// Tool-output detail settings shared by the TUI frontends.
    #[serde(default)]
    pub tool_output: ToolOutputFileConfig,
}

/// Behavioral configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BehaviorConfig {
    /// Explicit opt-in for terminal mouse capture. Defaults to off so the
    /// terminal keeps native text selection; the wheel still works through
    /// alternate scroll without capture.
    #[serde(default)]
    pub mouse_capture: bool,
}

/// Default copy target name for the config file. Lowercase in TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CopyTargetName {
    Message,
    Code,
}

/// Terminal channel configuration from the config file. Every field is
/// optional so a missing file or missing key falls back to the built-in
/// default.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Default copy target for explicit copy actions. Defaults to message.
    #[serde(default)]
    pub copy_target: Option<CopyTargetName>,
}

/// Mini display settings from the config file. Every field is optional so a
/// missing file or missing key falls back to the built-in default; the
/// resolution order is CLI flag, environment, config file, default.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MiniConfig {
    /// Assistant markdown presentation (`raw`, `plain`, `styled`).
    #[serde(default)]
    pub markdown: Option<MarkdownMode>,
    /// Show reasoning deltas on stderr. Defaults to shown.
    #[serde(default)]
    pub show_reasoning: Option<bool>,
    /// Show usage lines on stderr. Defaults to shown.
    #[serde(default)]
    pub show_usage: Option<bool>,
    /// Show tool lifecycle lines on stderr. Defaults to shown; failures
    /// are always shown regardless of this setting.
    #[serde(default)]
    pub show_tools: Option<bool>,
    /// Approval prompt timeout in seconds. Defaults to 120 when unset.
    #[serde(default)]
    pub approval_timeout_secs: Option<u64>,
}

/// Detail level name for the config file. Lowercase in TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolDetailLevelName {
    Low,
    Medium,
    High,
}

/// Tool-output detail settings from the config file. Every field is
/// optional so a missing file or missing key falls back to the built-in
/// default; the TUI maps these onto its runtime thresholds at startup and
/// the detail-level key cycles the level at runtime.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolOutputFileConfig {
    /// Master switch for tool-output previews. Defaults to on.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Generic preview head lines. Defaults to 10.
    #[serde(default)]
    pub preview_max_lines: Option<usize>,
    /// Generic preview grapheme budget. Defaults to 320.
    #[serde(default)]
    pub preview_max_chars: Option<usize>,
    /// Shell preview tail lines. Defaults to 10.
    #[serde(default)]
    pub shell_preview_lines: Option<usize>,
    /// Failure preview head lines of the error text. Defaults to 4.
    #[serde(default)]
    pub error_preview_lines: Option<usize>,
    /// Per-block stored excerpt cap in bytes. Defaults to 32768.
    #[serde(default)]
    pub block_max_bytes: Option<usize>,
    /// Default detail level. Defaults to medium. High expands retrieval
    /// bodies by default and can flood the transcript on large reads;
    /// reserve it for inspecting retrieval behavior.
    #[serde(default)]
    pub level: Option<ToolDetailLevelName>,
}

/// Configuration load error.
#[derive(Debug)]
pub enum ConfigError {
    /// The file exists but could not be read or parsed.
    Parse(String),
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "config error: {msg}"),
        }
    }
}

/// Configuration manager: loads a config file, falling back to defaults.
pub struct ConfigManager {
    config: AppConfig,
}

impl ConfigManager {
    /// Load the configuration from `path`; missing fields use defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&contents)
            .map_err(|e| ConfigError::Parse(format!("{}: {e}", path.display())))?;
        Ok(Self { config })
    }

    /// The loaded configuration (or defaults when not loaded from a file).
    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}

/// Determine the config file path following XDG conventions.
pub fn config_file_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("wf").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_use_defaults() {
        let parsed: AppConfig = toml::from_str("").unwrap();
        assert!(!parsed.behavior.mouse_capture);
    }

    #[test]
    fn mouse_capture_roundtrip_via_toml() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert!(!parsed.behavior.mouse_capture);
    }

    #[test]
    fn tool_output_detail_roundtrip_via_toml() {
        let parsed: AppConfig = toml::from_str(
            "[tool_output]\nenabled = false\npreview_max_lines = 5\nlevel = \"high\"\n",
        )
        .unwrap();
        assert_eq!(parsed.tool_output.enabled, Some(false));
        assert_eq!(parsed.tool_output.preview_max_lines, Some(5));
        assert_eq!(parsed.tool_output.level, Some(ToolDetailLevelName::High));
        assert_eq!(parsed.tool_output.preview_max_chars, None);
        let empty: AppConfig = toml::from_str("").unwrap();
        assert_eq!(empty.tool_output.enabled, None);
        assert_eq!(empty.tool_output.level, None);
    }

    #[test]
    fn terminal_copy_target_roundtrip_via_toml() {
        let parsed: AppConfig = toml::from_str("[terminal]\ncopy_target = \"code\"\n").unwrap();
        assert_eq!(parsed.terminal.copy_target, Some(CopyTargetName::Code));
        let empty: AppConfig = toml::from_str("").unwrap();
        assert_eq!(empty.terminal.copy_target, None);
        let message: AppConfig = toml::from_str("[terminal]\ncopy_target = \"message\"\n").unwrap();
        assert_eq!(message.terminal.copy_target, Some(CopyTargetName::Message));
    }

    #[test]
    fn mini_display_roundtrip_via_toml() {
        let parsed: AppConfig = toml::from_str(
            "[mini]\nmarkdown = \"plain\"\nshow_reasoning = false\napproval_timeout_secs = 30\n",
        )
        .unwrap();
        assert_eq!(
            parsed.mini.markdown,
            Some(crate::display::MarkdownMode::Plain)
        );
        assert_eq!(parsed.mini.show_reasoning, Some(false));
        assert_eq!(parsed.mini.approval_timeout_secs, Some(30));
        assert_eq!(parsed.mini.show_usage, None);
    }
}

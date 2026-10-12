//! Minimal user configuration for the CLI/TUI frontends.
//!
//! Only options with real consumers live here today (`mouse_capture`).
//! The file follows XDG conventions: `$XDG_CONFIG_HOME/wf/config.toml`
//! or `$HOME/.config/wf/config.toml`. A missing or unreadable file means
//! defaults; `load` never fails the caller on bad contents.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::display::MarkdownMode;

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Behavioral settings.
    #[serde(default)]
    pub behavior: BehaviorConfig,
    /// Mini interactive-form display settings.
    #[serde(default)]
    pub mini: MiniConfig,
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

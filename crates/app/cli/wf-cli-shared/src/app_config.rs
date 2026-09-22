//! Minimal user configuration for the CLI/TUI frontends.
//!
//! Only options with real consumers live here today (`mouse_capture`).
//! The file follows XDG conventions: `$XDG_CONFIG_HOME/wf/config.toml`
//! or `$HOME/.config/wf/config.toml`. A missing or unreadable file means
//! defaults; `load` never fails the caller on bad contents.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Behavioral settings.
    #[serde(default)]
    pub behavior: BehaviorConfig,
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
}

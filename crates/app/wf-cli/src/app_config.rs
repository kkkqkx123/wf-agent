//! Application configuration management with runtime updates and hot-reload.
//!
//! [`AppConfig`] holds all TUI configuration. [`ConfigManager`] provides
//! loading, saving, validation, and observer notification.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Keybinding overrides.
    pub keybindings: KeybindingConfig,
    /// Theme configuration.
    pub theme: ThemeConfig,
    /// Behavioral settings.
    pub behavior: BehaviorConfig,
    /// Editor settings.
    pub editor: EditorConfig,
}

/// Keybinding configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeybindingConfig {
    /// Global key overrides.
    pub global: HashMap<String, String>,
}

/// Theme configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Theme name (for display).
    pub name: String,
    /// Whether to use the probed theme.
    pub use_probed: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            use_probed: true,
        }
    }
}

/// Behavioral configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    /// Whether to auto-save state.
    pub auto_save: bool,
    /// Whether to confirm exit.
    pub confirm_exit: bool,
    /// Maximum history entries.
    pub history_size: usize,
    /// Scrollback line limit.
    pub scrollback_limit: usize,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            auto_save: true,
            confirm_exit: false,
            history_size: 200,
            scrollback_limit: 10_000,
        }
    }
}

/// Editor configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Tab size in spaces.
    pub tab_size: usize,
    /// Whether to show line numbers.
    pub line_numbers: bool,
    /// Whether to wrap long lines.
    pub word_wrap: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_size: 4,
            line_numbers: false,
            word_wrap: true,
        }
    }
}

/// Configuration update variants.
pub enum ConfigUpdate {
    Keybinding(KeybindingConfig),
    Theme(ThemeConfig),
    Behavior(BehaviorConfig),
    Editor(EditorConfig),
}

/// Configuration error types.
#[derive(Debug)]
pub enum ConfigError {
    /// Serialization/deserialization error.
    Serialize(String),
    /// IO error.
    Io(std::io::Error),
    /// Validation error.
    Validation(String),
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Serialize(e) => write!(f, "config serialization error: {e}"),
            ConfigError::Io(e) => write!(f, "config IO error: {e}"),
            ConfigError::Validation(e) => write!(f, "config validation error: {e}"),
        }
    }
}

/// Configuration observer trait.
pub trait ConfigWatcher: Send + Sync {
    /// Called when the configuration changes.
    fn on_config_changed(&self, config: &AppConfig);
}

/// Configuration manager: loads, saves, validates, and notifies observers.
pub struct ConfigManager {
    config: AppConfig,
    watchers: Vec<Arc<dyn ConfigWatcher>>,
    config_path: Option<PathBuf>,
}

impl ConfigManager {
    /// Create a new config manager with defaults and no config file.
    pub fn new() -> Self {
        Self {
            config: AppConfig::default(),
            watchers: Vec::new(),
            config_path: None,
        }
    }

    /// Load configuration from a file path.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let config = if path.exists() {
            let raw = std::fs::read_to_string(path)?;
            toml::from_str(&raw).map_err(|e| ConfigError::Serialize(e.to_string()))?
        } else {
            AppConfig::default()
        };
        Ok(Self {
            config,
            watchers: Vec::new(),
            config_path: Some(path.to_path_buf()),
        })
    }

    /// Save the current configuration to disk.
    pub fn save(&self) -> Result<(), ConfigError> {
        let Some(path) = &self.config_path else {
            return Ok(());
        };
        let toml = toml::to_string_pretty(&self.config)
            .map_err(|e| ConfigError::Serialize(e.to_string()))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml)?;
        Ok(())
    }

    /// Apply a configuration update, validate, and notify observers.
    pub fn update(&mut self, update: ConfigUpdate) -> Result<(), ConfigError> {
        match update {
            ConfigUpdate::Keybinding(kb) => self.config.keybindings = kb,
            ConfigUpdate::Theme(theme) => self.config.theme = theme,
            ConfigUpdate::Behavior(behavior) => self.config.behavior = behavior,
            ConfigUpdate::Editor(editor) => self.config.editor = editor,
        }

        self.validate()?;

        for watcher in &self.watchers {
            watcher.on_config_changed(&self.config);
        }

        self.save()?;
        Ok(())
    }

    /// Get a reference to the current configuration.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Get a mutable reference to the current configuration.
    pub fn config_mut(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    /// Register a configuration watcher.
    pub fn add_watcher(&mut self, watcher: Arc<dyn ConfigWatcher>) {
        self.watchers.push(watcher);
    }

    /// Validate the current configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.config.behavior.history_size == 0 {
            return Err(ConfigError::Validation(
                "history_size must be greater than 0".into(),
            ));
        }
        if self.config.behavior.scrollback_limit == 0 {
            return Err(ConfigError::Validation(
                "scrollback_limit must be greater than 0".into(),
            ));
        }
        if self.config.editor.tab_size == 0 || self.config.editor.tab_size > 16 {
            return Err(ConfigError::Validation(
                "tab_size must be between 1 and 16".into(),
            ));
        }
        Ok(())
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine the config file path following XDG conventions.
pub fn config_file_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("wf-cli").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestWatcher {
        called: Arc<AtomicBool>,
    }

    impl ConfigWatcher for TestWatcher {
        fn on_config_changed(&self, _config: &AppConfig) {
            self.called.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn default_config_is_valid() {
        let manager = ConfigManager::new();
        assert!(manager.validate().is_ok());
    }

    #[test]
    fn update_notifies_watchers() {
        let called = Arc::new(AtomicBool::new(false));
        let mut manager = ConfigManager::new();
        manager.add_watcher(Arc::new(TestWatcher {
            called: Arc::clone(&called),
        }));
        manager
            .update(ConfigUpdate::Behavior(BehaviorConfig {
                history_size: 100,
                ..Default::default()
            }))
            .unwrap();
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn validation_rejects_zero_history() {
        let mut manager = ConfigManager::new();
        let result = manager.update(ConfigUpdate::Behavior(BehaviorConfig {
            history_size: 0,
            ..Default::default()
        }));
        assert!(result.is_err());
    }

    #[test]
    fn validation_rejects_invalid_tab_size() {
        let mut manager = ConfigManager::new();
        let result = manager.update(ConfigUpdate::Editor(EditorConfig {
            tab_size: 0,
            ..Default::default()
        }));
        assert!(result.is_err());
    }

    #[test]
    fn roundtrip_via_toml() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.behavior.history_size, config.behavior.history_size);
    }
}

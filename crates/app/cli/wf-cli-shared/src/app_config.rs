//! Minimal user configuration for the CLI/TUI frontends.
//!
//! Only options with real consumers live here (`mouse_capture`,
//! `copy_target`). The file follows XDG conventions:
//! `$XDG_CONFIG_HOME/wf/tui.toml` or `$HOME/.config/wf/tui.toml`. A
//! missing or unreadable file means defaults; `load` never fails the caller
//! on bad contents.
//!
//! The kernel host layer owns `config.toml` in the same directory
//! (`storage`, `log_level`, `tool_approval`); this module never reads or
//! writes that file so frontend toggles cannot drop kernel keys.
//!
//! Backup scope is this file only. Keymap settings and theme files live
//! beside it but are owned by their own loaders; use file copy for those.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::display::MarkdownMode;

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Behavioral settings.
    #[serde(default)]
    pub behavior: BehaviorConfig,
    /// Terminal channel settings (clipboard copy target and terminal UX).
    #[serde(default)]
    pub terminal: TerminalConfig,
    /// Mini interactive-form display settings.
    #[serde(default)]
    pub mini: MiniConfig,
    /// Tool-output detail settings shared by the TUI frontends.
    #[serde(default)]
    pub tool_output: ToolOutputFileConfig,
    /// Appearance settings.
    #[serde(default)]
    pub appearance: AppearanceConfig,
    /// Rendering settings.
    #[serde(default)]
    pub rendering: RenderingConfig,
    /// Motion and animation settings.
    #[serde(default)]
    pub motion: MotionConfig,
    /// Notification settings.
    #[serde(default)]
    pub notifications: NotificationConfig,
    /// History and diagnostic settings.
    #[serde(default)]
    pub history: HistoryConfig,
}

/// Behavioral configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BehaviorConfig {
    /// Explicit opt-in for terminal mouse capture. Defaults to off so the
    /// terminal keeps native text selection; the wheel still works through
    /// alternate scroll without capture.
    #[serde(default)]
    pub mouse_capture: bool,
    /// Explicit opt-in for the main session scrollback scrollbar. Defaults
    /// to off to match the reference main session; takes effect instantly
    /// because the width change grades a full frame.
    #[serde(default)]
    pub session_scrollbar: bool,
    /// Explicit opt-in for the plugin session sidebar on the interactive
    /// screen. Defaults to off; takes effect instantly because the width
    /// change grades a full frame.
    #[serde(default)]
    pub sidebar: bool,
    #[serde(default)]
    pub approval_timeout_secs: Option<u64>,
}

/// Default copy target name for the config file. Lowercase in TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CopyTargetName {
    Message,
    Code,
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
    /// Maximum number of characters of a ghost suggestion that is painted
    /// on the terminal before an ellipsis. Defaults to 30 when unset.
    #[serde(default)]
    pub suggestion_display_limit: Option<usize>,
    /// Maximum number of prompt history entries retained in memory and on
    /// disk tail. Defaults to 200 when unset.
    #[serde(default)]
    pub history_limit: Option<usize>,
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
    #[serde(default)]
    pub tool_call_details: Option<bool>,
    #[serde(default)]
    pub compact_notifications: Option<bool>,
    #[serde(default)]
    pub show_bash_output: Option<bool>,
    #[serde(default)]
    pub show_agentgrep_output: Option<bool>,
    #[serde(default)]
    pub diff_line_wrap: Option<bool>,
}

/// Theme choice for the TUI. `auto` (default) probes the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    Auto,
    Dark,
    Light,
}

/// Cursor glyph shape. `default` preserves the terminal setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorStyleName {
    Block,
    Underline,
    Line,
    Default,
}

/// Motion mode requested by the user. `auto` keeps the environment-driven
/// default selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MotionModeName {
    Animated,
    Reduced,
    Static,
    Auto,
}

/// Performance tier override. `auto` keeps the capability-driven selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PerformanceTierName {
    Auto,
    Full,
    Reduced,
    Minimal,
}

/// Reasoning display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningDisplayModeName {
    Off,
    Full,
    Current,
}

/// Markdown block spacing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkdownSpacingModeName {
    Compact,
    Document,
}

/// LaTeX rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LatexRenderingModeName {
    None,
    Unicode,
    Image,
}

/// Mermaid/diagram display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagramModeName {
    None,
    Margin,
    Pinned,
}

/// Usage percentage wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageDisplayModeName {
    Left,
    Used,
}

/// Overscroll status line visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverscrollStatusModeName {
    Off,
    On,
    Overscroll,
}

/// Diff display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiffDisplayModeName {
    Auto,
    Stacked,
    Inline,
    Off,
    Full,
    Pinned,
    File,
}

/// Terminal-plus-UX settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Default copy target for explicit copy actions. Defaults to message.
    #[serde(default)]
    pub copy_target: Option<CopyTargetName>,
    /// Cursor shape. `None` keeps the terminal default (block).
    #[serde(default)]
    pub cursor_style: Option<CursorStyleName>,
    /// Whether the cursor blinks. `None` keeps the terminal default.
    #[serde(default)]
    pub cursor_blinking: Option<bool>,
    /// Scroll speed multiplier. `None` keeps the built-in default.
    #[serde(default)]
    pub scroll_speed: Option<f64>,
    /// Enable scroll acceleration. `None` keeps the built-in default.
    #[serde(default)]
    pub scroll_acceleration: Option<bool>,
    /// Enable keyboard enhancement probing/resolution. `None` keeps auto.
    #[serde(default)]
    pub keyboard_enhancement: Option<bool>,
}

/// Appearance settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default)]
    pub theme: Option<ThemeChoice>,
    /// Color role overrides as `#rrggbb` strings.
    #[serde(default)]
    pub colors: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub emoji: Option<bool>,
    #[serde(default)]
    pub centered: Option<bool>,
    #[serde(default)]
    pub idle_animation: Option<bool>,
    #[serde(default)]
    pub prompt_entry_animation: Option<bool>,
    #[serde(default)]
    pub prompt_preview: Option<bool>,
    #[serde(default)]
    pub keybinding_hints: Option<bool>,
}

/// Rendering settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderingConfig {
    #[serde(default)]
    pub show_thinking: Option<bool>,
    #[serde(default)]
    pub reasoning_display: Option<ReasoningDisplayModeName>,
    #[serde(default)]
    pub markdown_spacing: Option<MarkdownSpacingModeName>,
    #[serde(default)]
    pub latex_rendering: Option<LatexRenderingModeName>,
    #[serde(default)]
    pub diagram_mode: Option<DiagramModeName>,
    #[serde(default)]
    pub pin_images: Option<bool>,
    #[serde(default)]
    pub pin_todos: Option<bool>,
    #[serde(default)]
    pub usage_display: Option<UsageDisplayModeName>,
    #[serde(default)]
    pub overscroll_status: Option<OverscrollStatusModeName>,
    #[serde(default)]
    pub diff_mode: Option<DiffDisplayModeName>,
    #[serde(default)]
    pub diff_line_wrap: Option<bool>,
    #[serde(default)]
    pub tool_call_details: Option<bool>,
    #[serde(default)]
    pub compact_notifications: Option<bool>,
    #[serde(default)]
    pub show_bash_output: Option<bool>,
    #[serde(default)]
    pub show_agentgrep_output: Option<bool>,
    #[serde(default)]
    pub external_sessions: Option<bool>,
    #[serde(default)]
    pub active_sessions_manager: Option<bool>,
}

/// Motion and animation settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MotionConfig {
    #[serde(default)]
    pub mode: Option<MotionModeName>,
    #[serde(default)]
    pub disabled_animations: Vec<String>,
    #[serde(default)]
    pub animation_fps: Option<u32>,
    #[serde(default)]
    pub redraw_fps: Option<u32>,
    #[serde(default)]
    pub performance: Option<PerformanceTierName>,
    #[serde(default)]
    pub sidebar_refresh_interval_ms: Option<u64>,
}

/// Notification settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub turn_complete: Option<bool>,
    #[serde(default)]
    pub turn_complete_min_secs: Option<u64>,
    #[serde(default)]
    pub turn_complete_todo_min_secs: Option<u64>,
    #[serde(default)]
    pub terminal: Option<bool>,
    #[serde(default)]
    pub system: Option<bool>,
    #[serde(default)]
    pub bell: Option<bool>,
    #[serde(default)]
    pub background_only: Option<bool>,
    #[serde(default)]
    pub sound: Option<bool>,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub sound_pack: Option<String>,
}

/// History and diagnostic settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryConfig {
    #[serde(default)]
    pub replay_max_rows: Option<u64>,
    #[serde(default)]
    pub debug_socket: Option<bool>,
    #[serde(default)]
    pub slow_frame_ms: Option<u64>,
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

    /// Persist `config` to `path`, creating parent directories as needed.
    pub fn save(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(config)
            .map_err(|e| ConfigError::Parse(format!("{}: {e}", path.display())))?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Export the live config file to `dest`. A missing source exports
    /// defaults so backup never fails on a fresh machine.
    pub fn export_to(path: &Path, dest: &Path) -> Result<(), ConfigError> {
        let config = match Self::load(path) {
            Ok(manager) => manager.config().clone(),
            Err(_) => AppConfig::default(),
        };
        Self::save(dest, &config)
    }

    /// Import `src` over the live config file. The source is parsed first;
    /// a malformed source is rejected and `path` is left untouched.
    pub fn import_from(src: &Path, path: &Path) -> Result<(), ConfigError> {
        let manager = Self::load(src)?;
        Self::save(path, manager.config())
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
    Some(base.join("wf").join("tui.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_config_sections_roundtrip_via_toml() {
        let parsed: AppConfig = toml::from_str(
            r#"
[appearance]
theme = "light"
emoji = false
idle_animation = true

[rendering]
tool_call_details = true
compact_notifications = true
show_bash_output = true
show_agentgrep_output = false
usage_display = "used"
overscroll_status = "on"

[motion]
mode = "reduced"
performance = "minimal"
animation_fps = 30
redraw_fps = 60
disabled_animations = ["donut"]

[notifications]
enabled = true
turn_complete = true
bell = true

[history]
replay_max_rows = 1000
debug_socket = false
"#,
        )
        .unwrap();
        assert_eq!(parsed.appearance.theme, Some(ThemeChoice::Light));
        assert_eq!(parsed.appearance.emoji, Some(false));
        assert_eq!(parsed.rendering.tool_call_details, Some(true));
        assert_eq!(parsed.rendering.compact_notifications, Some(true));
        assert_eq!(parsed.rendering.show_bash_output, Some(true));
        assert_eq!(parsed.rendering.show_agentgrep_output, Some(false));
        assert_eq!(
            parsed.rendering.usage_display,
            Some(UsageDisplayModeName::Used)
        );
        assert_eq!(
            parsed.rendering.overscroll_status,
            Some(OverscrollStatusModeName::On)
        );
        assert_eq!(parsed.motion.mode, Some(MotionModeName::Reduced));
        assert_eq!(
            parsed.motion.performance,
            Some(PerformanceTierName::Minimal)
        );
        assert_eq!(parsed.motion.animation_fps, Some(30));
        assert_eq!(parsed.motion.redraw_fps, Some(60));
        assert_eq!(parsed.motion.disabled_animations, vec!["donut"]);
        assert_eq!(parsed.notifications.enabled, Some(true));
        assert_eq!(parsed.notifications.turn_complete, Some(true));
        assert_eq!(parsed.notifications.bell, Some(true));
        assert_eq!(parsed.history.replay_max_rows, Some(1000));
        assert_eq!(parsed.history.debug_socket, Some(false));
    }

    #[test]
    fn missing_fields_use_defaults() {
        let parsed: AppConfig = toml::from_str("").unwrap();
        assert!(!parsed.behavior.mouse_capture);
        assert!(!parsed.behavior.session_scrollbar);
    }

    #[test]
    fn mouse_capture_roundtrip_via_toml() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert!(!parsed.behavior.mouse_capture);
        assert!(!parsed.behavior.session_scrollbar);
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
    fn save_writes_loadable_config() {
        let dir = std::env::temp_dir().join("wf-config-save-test");
        let path = dir.join("nested").join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);
        let mut config = AppConfig::default();
        config.behavior.mouse_capture = true;
        config.tool_output.level = Some(ToolDetailLevelName::High);
        ConfigManager::save(&path, &config).expect("save writes config");
        let loaded = ConfigManager::load(&path).expect("save output loads");
        assert!(loaded.config().behavior.mouse_capture);
        assert_eq!(
            loaded.config().tool_output.level,
            Some(ToolDetailLevelName::High)
        );
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn export_missing_source_writes_loadable_defaults() {
        let dir = std::env::temp_dir().join("wf-config-export-defaults-test");
        let _ = std::fs::remove_dir_all(&dir);
        let missing = dir.join("absent").join("tui.toml");
        let dest = dir.join("backup.toml");
        ConfigManager::export_to(&missing, &dest).expect("export defaults");
        let loaded = ConfigManager::load(&dest).expect("export output loads");
        assert!(!loaded.config().behavior.mouse_capture);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_roundtrip_preserves_values() {
        let dir = std::env::temp_dir().join("wf-config-import-roundtrip-test");
        let _ = std::fs::remove_dir_all(&dir);
        let live = dir.join("tui.toml");
        let backup = dir.join("backup.toml");
        let mut config = AppConfig::default();
        config.behavior.mouse_capture = true;
        config.tool_output.level = Some(ToolDetailLevelName::High);
        ConfigManager::save(&live, &config).unwrap();
        ConfigManager::export_to(&live, &backup).unwrap();
        ConfigManager::save(&live, &AppConfig::default()).unwrap();
        assert!(
            !ConfigManager::load(&live)
                .unwrap()
                .config()
                .behavior
                .mouse_capture
        );
        ConfigManager::import_from(&backup, &live).expect("import valid backup");
        let restored = ConfigManager::load(&live).unwrap();
        assert!(restored.config().behavior.mouse_capture);
        assert_eq!(
            restored.config().tool_output.level,
            Some(ToolDetailLevelName::High)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_rejects_malformed_source_without_touching_live() {
        let dir = std::env::temp_dir().join("wf-config-import-reject-test");
        let _ = std::fs::remove_dir_all(&dir);
        let live = dir.join("tui.toml");
        let bad = dir.join("bad.toml");
        let mut config = AppConfig::default();
        config.behavior.mouse_capture = true;
        ConfigManager::save(&live, &config).unwrap();
        std::fs::create_dir_all(dir.clone()).unwrap();
        std::fs::write(&bad, "[behavior\nmouse_capture = not-toml").unwrap();
        assert!(ConfigManager::import_from(&bad, &live).is_err());
        assert!(
            ConfigManager::load(&live)
                .unwrap()
                .config()
                .behavior
                .mouse_capture
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

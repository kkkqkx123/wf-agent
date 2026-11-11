
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
    assert!(parsed.behavior.recap_enabled());
    assert_eq!(
        parsed.behavior.double_escape_action(),
        DoubleEscapeActionName::Tree
    );
}

#[test]
fn double_escape_action_roundtrip_via_toml() {
    let parsed: AppConfig =
        toml::from_str("[behavior]\ndouble_escape_action = \"fork\"\n").unwrap();
    assert_eq!(
        parsed.behavior.double_escape_action(),
        DoubleEscapeActionName::Fork
    );
    let off: AppConfig = toml::from_str("[behavior]\ndouble_escape_action = \"off\"\n").unwrap();
    assert_eq!(
        off.behavior.double_escape_action(),
        DoubleEscapeActionName::Off
    );
}

#[test]
fn recap_switch_roundtrip_via_toml() {
    let parsed: AppConfig = toml::from_str("[behavior]\nrecap = false\n").unwrap();
    assert!(!parsed.behavior.recap_enabled());
    let empty: AppConfig = toml::from_str("").unwrap();
    assert!(empty.behavior.recap_enabled());
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
    let parsed: AppConfig =
        toml::from_str("[tool_output]\nenabled = false\npreview_max_lines = 5\nlevel = \"high\"\n")
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
fn auto_title_mode_defaults_to_truncate() {
    let empty: AppConfig = toml::from_str("").unwrap();
    assert_eq!(empty.terminal.auto_title_mode(), AutoTitleMode::Truncate);
    assert_eq!(empty.terminal.title_profile(None), None);
    assert_eq!(
        empty.terminal.title_profile(Some("session-profile")),
        Some("session-profile".to_string())
    );
}

#[test]
fn auto_title_mode_roundtrip_via_toml() {
    let parsed: AppConfig =
        toml::from_str("[terminal]\nauto_title_mode = \"llm\"\ntitle_model = \"tiny\"\n").unwrap();
    assert_eq!(parsed.terminal.auto_title_mode(), AutoTitleMode::Llm);
    assert_eq!(
        parsed.terminal.title_profile(Some("session-profile")),
        Some("tiny".to_string())
    );
    let off: AppConfig = toml::from_str("[terminal]\nauto_title_mode = \"off\"\n").unwrap();
    assert_eq!(off.terminal.auto_title_mode(), AutoTitleMode::Off);
    assert_eq!(
        off.terminal.title_profile(Some("session-profile")),
        Some("session-profile".to_string())
    );
    let blank: AppConfig = toml::from_str("[terminal]\ntitle_model = \"  \"\n").unwrap();
    assert_eq!(
        blank.terminal.title_profile(Some("session-profile")),
        Some("session-profile".to_string())
    );
    assert_eq!(blank.terminal.title_profile(Some("  ")), None);
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

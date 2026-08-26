//! MCP settings loading and merging.
//!
//! Settings are merged from the global settings directory and project-level
//! `.wf/mcp.json` / `.agent/mcp.json` files, with project files taking
//! priority over the global file.

use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult};
use crate::loader;
use wf_types::tool::mcp_connection::{McpServerConfig, McpSettings};

pub const DEFAULT_MCP_SETTINGS_FILE: &str = "mcp-settings.json";
pub const PROJECT_MCP_FILE: &str = ".agent/mcp.json";
pub const PROJECT_WF_MCP_FILE: &str = ".wf/mcp.json";

/// Global settings file: `{settings_dir}/mcp-settings.json`.
pub fn get_global_mcp_settings_path(settings_dir: &Path) -> PathBuf {
    settings_dir.join(DEFAULT_MCP_SETTINGS_FILE)
}

/// Project-specific file: `{project_root}/.agent/mcp.json`.
pub fn get_project_mcp_path(project_root: &Path) -> PathBuf {
    project_root.join(PROJECT_MCP_FILE)
}

/// Project-specific file: `{project_root}/.wf/mcp.json` (highest priority).
pub fn get_project_wf_mcp_path(project_root: &Path) -> PathBuf {
    project_root.join(PROJECT_WF_MCP_FILE)
}

/// Project settings files in priority order (highest first).
pub fn get_project_mcp_paths(project_root: &Path) -> Vec<PathBuf> {
    vec![
        get_project_wf_mcp_path(project_root),
        get_project_mcp_path(project_root),
    ]
}

/// Load a single MCP settings file.
pub fn load_mcp_settings(file_path: &Path) -> ConfigResult<McpSettings> {
    if !file_path.exists() {
        return Err(ConfigError::NotFound(format!(
            "MCP settings file not found: {}",
            file_path.display()
        )));
    }
    loader::load_config_file_sync::<McpSettings>(file_path)
}

/// Load and merge MCP settings from the global directory and all project
/// files. Priority chain (highest first): `.wf/mcp.json` > `.agent/mcp.json`
/// > global `mcp-settings.json`. Missing files are skipped.
pub fn load_and_merge_mcp_settings(
    settings_dir: &Path,
    project_root: &Path,
) -> ConfigResult<McpSettings> {
    let global_path = get_global_mcp_settings_path(settings_dir);
    let project_paths = get_project_mcp_paths(project_root);

    let mut merged: std::collections::HashMap<String, McpServerConfig> =
        match load_mcp_settings(&global_path) {
            Ok(settings) => settings.mcp_servers,
            Err(_) => std::collections::HashMap::new(),
        };

    // Apply project layers in ascending priority order so that higher
    // priority files (.wf/mcp.json) override lower ones (.agent/mcp.json).
    for path in project_paths.iter().rev() {
        if let Ok(settings) = load_mcp_settings(path) {
            for (name, config) in settings.mcp_servers {
                merged.insert(name, config);
            }
        }
    }

    Ok(McpSettings {
        mcp_servers: merged,
    })
}

/// Load merged settings, returning an empty settings object when no source
/// file exists (used at bootstrap when MCP is not configured).
pub fn try_load_and_merge_mcp_settings(settings_dir: &Path, project_root: &Path) -> McpSettings {
    load_and_merge_mcp_settings(settings_dir, project_root).unwrap_or_default()
}

/// Write MCP settings to a JSON file.
pub fn write_mcp_settings(file_path: &Path, settings: &McpSettings) -> ConfigResult<()> {
    let content = serde_json::to_string_pretty(settings)?;
    std::fs::write(file_path, content)?;
    Ok(())
}

/// Ensure an MCP settings file exists, creating a default one if absent.
/// Returns `true` if the file was created, `false` if it already existed.
pub fn ensure_mcp_settings_file(file_path: &Path) -> ConfigResult<bool> {
    if file_path.exists() {
        return Ok(false);
    }
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_mcp_settings(file_path, &McpSettings::default())?;
    Ok(true)
}

/// Default MCP preset directory: `{project_root}/configs/mcp`.
pub fn get_default_mcp_preset_dir(project_root: &Path) -> PathBuf {
    project_root.join("configs").join("mcp")
}

/// Load MCP settings from a preset by name (preset mode).
///
/// Resolution: load `configs/mcp/index.json` via the shared preset loader,
/// match `preset_name` to a preset file by filename, then parse it as
/// `McpSettings`.
pub fn load_mcp_preset_settings(base_dir: &Path, preset_name: &str) -> ConfigResult<McpSettings> {
    let resolved = crate::preset::resolve_preset_index(base_dir)?;
    let entry = crate::preset::find_preset_by_name(&resolved, preset_name).ok_or_else(|| {
        let available = crate::preset::list_preset_names(&resolved).join(", ");
        ConfigError::NotFound(format!(
            "MCP preset \"{preset_name}\" not found in {}. Available presets: {}",
            base_dir.display(),
            if available.is_empty() {
                "(none)".to_string()
            } else {
                available
            }
        ))
    })?;
    crate::preset::load_single_file_preset::<McpSettings>(entry)
}

/// Load MCP settings with preset support.
///
/// Tries preset mode first (when `configs/mcp/index.json` exists and a preset
/// name is provided), then falls back to the legacy global/project chain.
/// In preset mode the merge chain is: project overrides > global > preset base.
pub fn load_and_merge_mcp_settings_with_preset(
    settings_dir: &Path,
    project_root: &Path,
    preset_name: Option<&str>,
) -> ConfigResult<McpSettings> {
    let preset_dir = get_default_mcp_preset_dir(project_root);
    let index_path = preset_dir.join(crate::preset::INDEX_FILE_NAME);

    if !index_path.exists() {
        return load_and_merge_mcp_settings(settings_dir, project_root);
    }

    let base_settings: Option<McpSettings> = match preset_name {
        Some(name) => match load_mcp_preset_settings(&preset_dir, name) {
            Ok(settings) => Some(settings),
            Err(_) => return load_and_merge_mcp_settings(settings_dir, project_root),
        },
        None => None,
    };

    let global_settings = load_mcp_settings(&get_global_mcp_settings_path(settings_dir)).ok();
    let project_paths = get_project_mcp_paths(project_root);

    let mut merged: std::collections::HashMap<String, McpServerConfig> =
        if let Some(base) = &base_settings {
            base.mcp_servers.clone()
        } else if let Some(global) = &global_settings {
            global.mcp_servers.clone()
        } else {
            std::collections::HashMap::new()
        };

    // Global overrides the preset base when a preset was used.
    if base_settings.is_some() {
        if let Some(global) = &global_settings {
            for (name, config) in &global.mcp_servers {
                merged.insert(name.clone(), config.clone());
            }
        }
    }

    // Project layers in ascending priority order so that higher priority
    // files (.wf/mcp.json) override lower ones (.agent/mcp.json).
    for path in project_paths.iter().rev() {
        if let Ok(settings) = load_mcp_settings(path) {
            for (name, config) in settings.mcp_servers {
                merged.insert(name, config);
            }
        }
    }

    Ok(McpSettings {
        mcp_servers: merged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::tool::mcp_connection::McpStdioConfig;

    fn write_json(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn test_global_and_project_merge_priority() {
        let root = std::env::temp_dir().join(format!("wf-mcp-loader-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");

        write_json(
            &root.join("mcp-settings.json"),
            r#"{"mcpServers": {"a": {"type": "stdio", "command": "global-a"}, "b": {"type": "stdio", "command": "global-b"}}}"#,
        );
        write_json(
            &project.join(".agent/mcp.json"),
            r#"{"mcpServers": {"b": {"type": "stdio", "command": "agent-b"}, "c": {"type": "stdio", "command": "agent-c"}}}"#,
        );
        write_json(
            &project.join(".wf/mcp.json"),
            r#"{"mcpServers": {"b": {"type": "stdio", "command": "wf-b"}}}"#,
        );

        let settings = load_and_merge_mcp_settings(&root, &project).unwrap();
        assert_eq!(settings.mcp_servers.len(), 3);
        // .wf overrides .agent overrides global for the same key.
        let b = match &settings.mcp_servers["b"] {
            McpServerConfig::Stdio(c) => &c.command,
            _ => unreachable!(),
        };
        assert_eq!(b, "wf-b");
        let a = match &settings.mcp_servers["a"] {
            McpServerConfig::Stdio(c) => &c.command,
            _ => unreachable!(),
        };
        assert_eq!(a, "global-a");
        let c = match &settings.mcp_servers["c"] {
            McpServerConfig::Stdio(c) => &c.command,
            _ => unreachable!(),
        };
        assert_eq!(c, "agent-c");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_missing_sources_yield_empty_settings() {
        let root = std::env::temp_dir().join(format!("wf-mcp-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let settings = try_load_and_merge_mcp_settings(&root, &root);
        assert!(settings.mcp_servers.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_streamable_http_tag_parsing() {
        let root = std::env::temp_dir().join(format!("wf-mcp-tag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_json(
            &root.join("mcp-settings.json"),
            r#"{"mcpServers": {"s": {"type": "streamable-http", "url": "http://localhost:8080/mcp"}}}"#,
        );
        let settings = load_mcp_settings(&root.join("mcp-settings.json")).unwrap();
        assert!(matches!(
            settings.mcp_servers["s"],
            McpServerConfig::StreamableHttp(_)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_ensure_and_write_mcp_settings() {
        let root = std::env::temp_dir().join(format!("wf-mcp-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("nested").join("mcp-settings.json");

        assert!(ensure_mcp_settings_file(&path).unwrap());
        assert!(!ensure_mcp_settings_file(&path).unwrap());
        let settings = load_mcp_settings(&path).unwrap();
        assert!(settings.mcp_servers.is_empty());

        let settings = McpSettings {
            mcp_servers: [(
                "a".to_string(),
                McpServerConfig::Stdio(McpStdioConfig {
                    base: wf_types::tool::mcp_connection::McpServerConfigBase {
                        disabled: None,
                        timeout: None,
                        always_allow: None,
                        disabled_tools: None,
                        lifecycle: None,
                        idle_timeout: None,
                        health_check_interval: None,
                    },
                    command: "cmd".to_string(),
                    args: None,
                    cwd: None,
                    env: None,
                }),
            )]
            .into_iter()
            .collect(),
        };
        write_mcp_settings(&path, &settings).unwrap();
        let loaded = load_mcp_settings(&path).unwrap();
        assert!(loaded.mcp_servers.contains_key("a"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_mcp_preset_priority_chain() {
        let root = std::env::temp_dir().join(format!("wf-mcp-preset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");
        let preset_dir = project.join("configs").join("mcp");
        std::fs::create_dir_all(&preset_dir).unwrap();

        // Preset base + global override + project override.
        write_json(
            &preset_dir.join("index.json"),
            r#"{"version": "1.0", "type": "mcp_presets", "paths": ["./*.json"]}"#,
        );
        write_json(
            &preset_dir.join("default.json"),
            r#"{"mcpServers": {"a": {"type": "stdio", "command": "preset-a"}, "b": {"type": "stdio", "command": "preset-b"}, "c": {"type": "stdio", "command": "preset-c"}}}"#,
        );
        write_json(
            &root.join("mcp-settings.json"),
            r#"{"mcpServers": {"b": {"type": "stdio", "command": "global-b"}}}"#,
        );
        write_json(
            &project.join(".wf/mcp.json"),
            r#"{"mcpServers": {"c": {"type": "stdio", "command": "wf-c"}}}"#,
        );

        let settings =
            load_and_merge_mcp_settings_with_preset(&root, &project, Some("default")).unwrap();
        assert_eq!(settings.mcp_servers.len(), 3);
        // Preset-only server keeps preset value.
        let a = match &settings.mcp_servers["a"] {
            McpServerConfig::Stdio(c) => &c.command,
            _ => unreachable!(),
        };
        assert_eq!(a, "preset-a");
        // Global overrides preset base.
        let b = match &settings.mcp_servers["b"] {
            McpServerConfig::Stdio(c) => &c.command,
            _ => unreachable!(),
        };
        assert_eq!(b, "global-b");
        // Project overrides global.
        let c = match &settings.mcp_servers["c"] {
            McpServerConfig::Stdio(c) => &c.command,
            _ => unreachable!(),
        };
        assert_eq!(c, "wf-c");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_mcp_preset_missing_falls_back_to_legacy() {
        let root = std::env::temp_dir().join(format!("wf-mcp-presetmiss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");
        let preset_dir = project.join("configs").join("mcp");
        std::fs::create_dir_all(&preset_dir).unwrap();
        write_json(
            &preset_dir.join("index.json"),
            r#"{"version": "1.0", "type": "mcp_presets", "paths": ["./*.json"]}"#,
        );

        // No preset index-less fallback: index exists but preset missing →
        // legacy chain (global only).
        write_json(
            &root.join("mcp-settings.json"),
            r#"{"mcpServers": {"g": {"type": "stdio", "command": "global-g"}}}"#,
        );
        let settings =
            load_and_merge_mcp_settings_with_preset(&root, &project, Some("nope")).unwrap();
        assert_eq!(settings.mcp_servers.len(), 1);
        assert!(settings.mcp_servers.contains_key("g"));

        // No index at all → legacy chain.
        let root2 = std::env::temp_dir().join(format!("wf-mcp-presetmiss2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root2);
        let project2 = root2.join("proj");
        write_json(
            &root2.join("mcp-settings.json"),
            r#"{"mcpServers": {"g": {"type": "stdio", "command": "global-g"}}}"#,
        );
        let settings =
            load_and_merge_mcp_settings_with_preset(&root2, &project2, Some("default")).unwrap();
        assert_eq!(settings.mcp_servers.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&root2);
    }
}

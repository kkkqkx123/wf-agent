//! MCP settings loading and merging.
//!
//! Settings are merged from the global settings directory and the project-level
//! `.wf/mcp.json` file, with the project file taking precedence over the
//! global file.

use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult};
use crate::layout;
use crate::loader;
use wf_types::tool::mcp_connection::{McpServerConfig, McpSettings};

pub const DEFAULT_MCP_SETTINGS_FILE: &str = "mcp-settings.json";

/// Global settings file: `{settings_dir}/mcp-settings.json`.
pub fn get_global_mcp_settings_path(settings_dir: &Path) -> PathBuf {
    settings_dir.join(DEFAULT_MCP_SETTINGS_FILE)
}

/// Project-specific file: `{project_root}/.wf/mcp.json` (highest precedence).
pub fn get_project_mcp_path(project_root: &Path) -> PathBuf {
    project_root.join(layout::PROJECT_WF_DIR).join("mcp.json")
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

/// Load a single MCP settings file, logging a warning when the file exists
/// but cannot be parsed (missing files are expected and stay silent).
fn load_mcp_settings_warn(file_path: &Path) -> Option<McpSettings> {
    match load_mcp_settings(file_path) {
        Ok(settings) => Some(settings),
        Err(e) => {
            if file_path.exists() {
                tracing::warn!(error = %e, "ignoring unparseable MCP settings file");
            }
            None
        }
    }
}

/// Load and merge MCP settings from the explicit global directory and the
/// project file. Precedence chain (highest first): `.wf/mcp.json` >
/// global `mcp-settings.json`. A `None` global dir runs project-only so a
/// service without shared settings still honors its project file. Missing
/// files are skipped; files that exist but fail to parse produce a warning
/// and are skipped.
pub fn load_and_merge_mcp_settings(
    settings_dir: Option<&Path>,
    project_root: &Path,
) -> ConfigResult<McpSettings> {
    let project_path = get_project_mcp_path(project_root);

    let mut merged: std::collections::HashMap<String, McpServerConfig> = settings_dir
        .and_then(|dir| load_mcp_settings_warn(&get_global_mcp_settings_path(dir)))
        .map(|settings| settings.mcp_servers)
        .unwrap_or_default();

    // The project layer overrides the global layer for the same key.
    if let Some(settings) = load_mcp_settings_warn(&project_path) {
        for (name, config) in settings.mcp_servers {
            merged.insert(name, config);
        }
    }

    Ok(McpSettings {
        mcp_servers: merged,
    })
}

/// Load merged settings, returning an empty settings object when no source
/// file exists (used at bootstrap when MCP is not configured).
pub fn try_load_and_merge_mcp_settings(
    settings_dir: Option<&Path>,
    project_root: &Path,
) -> McpSettings {
    load_and_merge_mcp_settings(settings_dir, project_root).unwrap_or_default()
}

/// Write MCP settings to a JSON file atomically (temporary sibling plus
/// rename), so a full disk leaves the previous file intact instead of a
/// truncated one.
pub fn write_mcp_settings(file_path: &Path, settings: &McpSettings) -> ConfigResult<()> {
    let content = serde_json::to_string_pretty(settings)?;
    wf_common::fs::write_atomic_sync(file_path, content.as_bytes())?;
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
    fn test_global_and_project_merge_precedence() {
        let root = std::env::temp_dir().join(format!("wf-mcp-loader-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");

        write_json(
            &root.join("mcp-settings.json"),
            r#"{"mcpServers": {"a": {"type": "stdio", "command": "global-a"}, "b": {"type": "stdio", "command": "global-b"}}}"#,
        );
        write_json(
            &project.join(".wf/mcp.json"),
            r#"{"mcpServers": {"b": {"type": "stdio", "command": "wf-b"}, "c": {"type": "stdio", "command": "wf-c"}}}"#,
        );

        let settings = load_and_merge_mcp_settings(Some(root.as_path()), &project).unwrap();
        assert_eq!(settings.mcp_servers.len(), 3);
        // .wf overrides global for the same key.
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
        assert_eq!(c, "wf-c");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_missing_sources_yield_empty_settings() {
        let root = std::env::temp_dir().join(format!("wf-mcp-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let settings = try_load_and_merge_mcp_settings(Some(root.as_path()), &root);
        assert!(settings.mcp_servers.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn project_only_settings_load_without_global_dir() {
        let root = std::env::temp_dir().join(format!("wf-mcp-project-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");

        write_json(
            &project.join(".wf/mcp.json"),
            r#"{"mcpServers": {"p": {"type": "stdio", "command": "proj-cmd"}}}"#,
        );

        let settings = load_and_merge_mcp_settings(None, &project).unwrap();
        assert_eq!(settings.mcp_servers.len(), 1);
        assert!(settings.mcp_servers.contains_key("p"));

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
}

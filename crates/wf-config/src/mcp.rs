//! MCP settings loading and merging.
//!
//! Mirrors the TS `config-processor/mcp-settings-loader` behavior:
//! settings are merged from the global settings directory and project-level
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

#[cfg(test)]
mod tests {
    use super::*;

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
}

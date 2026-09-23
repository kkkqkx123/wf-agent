//! User-level config file layer (global + project dotfile).
//!
//! Loads `$XDG_CONFIG_HOME/wf/config.toml` then `<project>/.wf/config.toml`
//! (project wins) leniently: absent files yield defaults and a malformed
//! file is skipped with a warning instead of failing bootstrap. This layer
//! covers only the small set of host knobs operators expect in a dotfile
//! (`storage`, `log_level`, `tool_approval`); infrastructure presets stay in
//! the orchestrator, which applies on top with higher priority.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::warn;

use wf_types::config::storage::StorageConfig;
use wf_types::config::tool_approval::ToolApprovalConfig;

/// Host knobs readable from the user/project dotfile layer. Every field is
/// optional: `None` means "not set, keep the caller value".
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FileLayerConfig {
    pub storage: Option<StorageConfig>,
    pub log_level: Option<String>,
    pub tool_approval: Option<ToolApprovalConfig>,
}

/// User-level config directory following XDG conventions
/// (`$XDG_CONFIG_HOME/wf` or `$HOME/.config/wf`); `None` when no home is
/// discoverable.
pub fn user_config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("wf"))
}

/// Layer paths in precedence order (low to high): global file then project
/// file, so the project dotfile wins.
pub fn user_layer_paths(project_root: &Path) -> Vec<PathBuf> {
    layer_paths(user_config_dir(), project_root)
}

fn layer_paths(global_config_dir: Option<PathBuf>, project_root: &Path) -> Vec<PathBuf> {
    let project_file = project_root
        .join(crate::layout::PROJECT_WF_DIR)
        .join("config.toml");
    let global_file = global_config_dir.map(|dir| dir.join("config.toml"));

    [global_file, Some(project_file)]
        .into_iter()
        .flatten()
        .collect()
}

/// Load the user config-file layer (global then project, project wins).
/// Returns defaults when no file exists; a malformed file is skipped with a
/// warning rather than failing the caller.
pub fn load_user_file_layer(project_root: &Path) -> FileLayerConfig {
    load_file_layer(user_config_dir(), project_root)
}

fn load_file_layer(global_config_dir: Option<PathBuf>, project_root: &Path) -> FileLayerConfig {
    let paths = layer_paths(global_config_dir, project_root);
    if paths.is_empty() {
        return FileLayerConfig::default();
    }
    let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    match crate::layered::load_layered_config_sync::<FileLayerConfig>(&path_refs) {
        Ok(layer) => layer,
        Err(e) => {
            warn!(error = %e, "failed to load user config layer; keeping defaults");
            FileLayerConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_layer_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        // Explicit global dir with no config.toml: no env read, no cross-test race.
        let global = dir.path().join("config-home");
        let layer = load_file_layer(Some(global), &dir.path().join("project"));
        assert!(layer.storage.is_none());
        assert!(layer.log_level.is_none());
        assert!(layer.tool_approval.is_none());
    }

    #[test]
    fn project_layer_wins_over_global() {
        let global_dir = tempfile::tempdir().unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(global_dir.path().join("wf")).unwrap();
        std::fs::create_dir_all(project_dir.path().join(".wf")).unwrap();
        std::fs::write(
            global_dir.path().join("wf").join("config.toml"),
            "log_level = \"debug\"\n",
        )
        .unwrap();
        std::fs::write(
            project_dir.path().join(".wf").join("config.toml"),
            "log_level = \"info\"\n",
        )
        .unwrap();

        let layer = load_file_layer(Some(global_dir.path().to_path_buf()), project_dir.path());
        assert_eq!(layer.log_level.as_deref(), Some("info"));
    }
}

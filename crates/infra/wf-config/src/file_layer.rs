//! User-level config file layer (explicit global plus project dotfile).
//!
//! Loads the caller-provided global `config.toml` then
//! `<project>/.wf/config.toml` (project wins) leniently: absent files yield
//! defaults and a malformed file is skipped with a warning instead of
//! failing bootstrap. Workstation CLIs pass `user_config_dir()` as the
//! global dir; services pass an explicit `--settings-dir` or `None` to
//! disable the ambient layer entirely. This layer
//! covers only the small set of host knobs operators expect in a dotfile
//! (`storage`, `log_level`, `log_format`, `tool_approval`); fixed infrastructure files stay in
//! the orchestrator, which applies on top with higher priority. File
//! checkpoint and sandbox settings are deliberately excluded here so the
//! dotfile layer can never shadow the fixed infrastructure files with a
//! partial sibling value.
//!
//! The CLI frontend display knobs live in the sibling `tui.toml` file and
//! are owned by the CLI shared config module; this layer ignores those
//! sections so frontend edits never disturb kernel keys.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::ConfigError;

use wf_types::config::storage::StorageConfig;
use wf_types::config::tool_approval::ToolApprovalConfig;

/// Host knobs readable from the user/project dotfile layer. Every field is
/// optional: `None` means "not set, keep the caller value".
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FileLayerConfig {
    pub storage: Option<StorageConfig>,
    pub log_level: Option<String>,
    pub log_format: Option<String>,
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

/// Layer paths in precedence order (low to high): explicit global file
/// then project file, so the project dotfile wins. A `None` global dir
/// disables the ambient layer entirely: only the project file is read.
/// Callers resolve the global dir themselves (`user_config_dir` for
/// workstation CLIs, an explicit `--settings-dir` for services) so loaders
/// never read process environment on their own.
pub fn user_layer_paths(global_config_dir: Option<PathBuf>, project_root: &Path) -> Vec<PathBuf> {
    layer_paths(global_config_dir, project_root)
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

/// Load result with warnings that the caller emits after installing logging.
#[derive(Debug, Clone, Default)]
pub struct UserFileLayerLoad {
    pub config: FileLayerConfig,
    pub warnings: Vec<String>,
}

/// Load the user config-file layer (explicit global then project, project
/// wins). Returns defaults when no file exists; a malformed file is skipped
/// and reported through `warnings` instead of failing the caller. Passing
/// `None` as the global dir reads the project file only.
pub fn load_user_file_layer(
    global_config_dir: Option<PathBuf>,
    project_root: &Path,
) -> UserFileLayerLoad {
    load_file_layer(global_config_dir, project_root)
}

fn load_file_layer(global_config_dir: Option<PathBuf>, project_root: &Path) -> UserFileLayerLoad {
    let paths = layer_paths(global_config_dir, project_root);
    if paths.is_empty() {
        return UserFileLayerLoad::default();
    }
    let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    match crate::layered::load_layered_config_sync::<FileLayerConfig>(&path_refs) {
        Ok(config) => UserFileLayerLoad {
            config,
            warnings: Vec::new(),
        },
        Err(ConfigError::NotFound(_)) => UserFileLayerLoad::default(),
        Err(e) => UserFileLayerLoad {
            config: FileLayerConfig::default(),
            warnings: vec![format!(
                "failed to load user config layer from {}; keeping defaults: {e}",
                display_paths(&paths)
            )],
        },
    }
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
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
        assert!(layer.config.storage.is_none());
        assert!(layer.config.log_level.is_none());
        assert!(layer.config.log_format.is_none());
        assert!(layer.config.tool_approval.is_none());
        assert!(layer.warnings.is_empty());
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
            "log_level = \"info\"\nlog_format = \"json\"\n",
        )
        .unwrap();

        let layer = load_file_layer(Some(global_dir.path().to_path_buf()), project_dir.path());
        assert_eq!(layer.config.log_level.as_deref(), Some("info"));
        assert_eq!(layer.config.log_format.as_deref(), Some("json"));
        assert!(layer.warnings.is_empty());
    }

    #[test]
    fn absent_global_dir_reads_project_only() {
        let project_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_dir.path().join(".wf")).unwrap();
        std::fs::write(
            project_dir.path().join(".wf").join("config.toml"),
            "log_level = \"info\"\n",
        )
        .unwrap();

        let layer = load_user_file_layer(None, project_dir.path());
        assert_eq!(layer.config.log_level.as_deref(), Some("info"));
        assert!(layer.warnings.is_empty());
    }

    #[test]
    fn malformed_layer_reports_warning() {
        let global_dir = tempfile::tempdir().unwrap();
        let project_dir = tempfile::tempdir().unwrap();
        std::fs::write(global_dir.path().join("config.toml"), "log_level = [\n").unwrap();

        let layer = load_file_layer(Some(global_dir.path().to_path_buf()), project_dir.path());
        assert!(layer.config.log_level.is_none());
        assert!(layer.config.log_format.is_none());
        assert_eq!(layer.warnings.len(), 1);
    }
}

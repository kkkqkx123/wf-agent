//! Centralized filesystem layout for the configuration subsystem.
//!
//! Every well-known directory / file name used when locating configuration
//! sources lives here so that path conventions are defined in exactly one
//! place. Domain modules (mcp, skill, preset, orchestrator, ...) compose
//! these constants instead of hardcoding their own path fragments.

/// Project-layer file root: `{project_root}/.wf`. Holds per-project
/// overrides; travels with the business repository, never with the
/// release artifact.
pub const PROJECT_WF_DIR: &str = ".wf";

/// Directory name of configuration documentation samples. Files below
/// are never loaded; see `config-examples/README.md`.
pub const EXAMPLES_DIR: &str = "config-examples";

/// Index file name shared by index-based config families.
pub const INDEX_FILE_NAME: &str = "index.json";

/// Directory name of each effective config family. Every family resolves
/// from the user settings layer and the project dotfile layer; samples
/// live under `config-examples/` and have no family constant so nothing
/// resolves them by accident.
pub mod family {
    /// Infrastructure domain files with fixed filenames, resolved from
    /// `{settings_dir}/infrastructure/` and `{project_root}/.wf/infrastructure/`.
    pub const INFRASTRUCTURE: &str = "infrastructure";
    /// Server transport and middleware files, resolved from
    /// `{settings_dir}/server/` and `{project_root}/.wf/server/`.
    pub const SERVER: &str = "server";
    /// LLM authentication declarations (user-level `llm-auth` under the
    /// project dotfile directory or the explicit settings directory).
    pub const LLM_AUTH: &str = "llm-auth";
}

/// Default global settings file names inside `{settings_dir}`.
pub mod settings_file {
    /// Global MCP settings (`mcp-settings.json`).
    pub const MCP: &str = "mcp-settings.json";
    /// Global skill settings (`skill-settings.json`).
    pub const SKILL: &str = "skill-settings.json";
}

/// Volatile machine-local metadata under `{project_root}/.wf/meta`:
/// session caches, diagnostics and other files that must never be
/// version-controlled or enter checkpoint scans.
pub const META_DIR: &str = "meta";

/// Volatile spill output under `{project_root}/.wf/tmp`: oversized
/// command output and other regenerable artifacts.
pub const TMP_DIR: &str = "tmp";

/// Content of the `.gitignore` maintained inside each volatile directory:
/// ignore everything but keep the ignore file itself tracked-ignored.
pub const VOLATILE_GITIGNORE_CONTENT: &str = "*\n!.gitignore\n";

/// Name of the ignore file maintained inside volatile directories.
pub const GITIGNORE_FILE: &str = ".gitignore";

/// Ensure the volatile `.wf` subdirectories exist with their internal
/// ignore files. Only `{project_root}/.wf/{meta,tmp}/.gitignore` are
/// created or written; the outer workspace `.gitignore` is never touched
/// so tracked `.wf` configuration keeps its sync behavior. A present
/// ignore file with custom content is left alone (never force-overwritten).
pub fn ensure_project_wf_volatile_dirs(project_root: &std::path::Path) -> std::io::Result<()> {
    for subdir in [META_DIR, TMP_DIR] {
        let dir = project_root.join(PROJECT_WF_DIR).join(subdir);
        std::fs::create_dir_all(&dir)?;
        let ignore = dir.join(GITIGNORE_FILE);
        if std::fs::read(&ignore).is_err() {
            std::fs::write(&ignore, VOLATILE_GITIGNORE_CONTENT)?;
        }
    }
    Ok(())
}

/// Project-layer infrastructure directory:
/// `{project_root}/.wf/infrastructure`. Holds per-project domain
/// overrides; wins over the settings layer.
pub fn project_infra_dir(project_root: &std::path::Path) -> std::path::PathBuf {
    project_root
        .join(PROJECT_WF_DIR)
        .join(family::INFRASTRUCTURE)
}

/// Global settings-layer infrastructure directory:
/// `{settings_dir}/infrastructure`. Holds per-operator domain overrides.
pub fn settings_infra_dir(settings_dir: &std::path::Path) -> std::path::PathBuf {
    settings_dir.join(family::INFRASTRUCTURE)
}

/// Project-layer server family directory: `{project_root}/.wf/server`.
/// Wins over the settings layer.
pub fn project_server_dir(project_root: &std::path::Path) -> std::path::PathBuf {
    project_root.join(PROJECT_WF_DIR).join(family::SERVER)
}

/// Global settings-layer server family directory: `{settings_dir}/server`.
pub fn settings_server_dir(settings_dir: &std::path::Path) -> std::path::PathBuf {
    settings_dir.join(family::SERVER)
}

/// Documentation samples are never loaded: any file whose name contains
/// `.example.` is skipped by index expansion and family globs so checked-in
/// samples cannot leak into a running configuration.
pub fn is_example_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.contains(".example."))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volatile_dirs_are_created_with_internal_ignores() {
        let dir = tempfile::tempdir().unwrap();
        ensure_project_wf_volatile_dirs(dir.path()).unwrap();
        for subdir in [META_DIR, TMP_DIR] {
            let ignore = dir
                .path()
                .join(PROJECT_WF_DIR)
                .join(subdir)
                .join(GITIGNORE_FILE);
            assert_eq!(
                std::fs::read_to_string(&ignore).unwrap(),
                VOLATILE_GITIGNORE_CONTENT
            );
        }
        assert!(!dir.path().join(".gitignore").exists());
        // Second run is idempotent.
        ensure_project_wf_volatile_dirs(dir.path()).unwrap();
    }

    #[test]
    fn existing_custom_ignore_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let meta = dir.path().join(PROJECT_WF_DIR).join(META_DIR);
        std::fs::create_dir_all(&meta).unwrap();
        std::fs::write(meta.join(GITIGNORE_FILE), "custom\n").unwrap();
        ensure_project_wf_volatile_dirs(dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(meta.join(GITIGNORE_FILE)).unwrap(),
            "custom\n"
        );
        assert_eq!(
            std::fs::read_to_string(
                dir.path()
                    .join(PROJECT_WF_DIR)
                    .join(TMP_DIR)
                    .join(GITIGNORE_FILE)
            )
            .unwrap(),
            VOLATILE_GITIGNORE_CONTENT
        );
    }

    #[test]
    fn example_files_are_recognized_by_name() {
        use std::path::Path;
        assert!(is_example_path(Path::new(
            "llm-auth/api-key-provider.auth.example.toml"
        )));
        assert!(is_example_path(Path::new(
            "workflows/code.review.example.toml"
        )));
        assert!(!is_example_path(Path::new("llm-auth/custom.auth.toml")));
        assert!(!is_example_path(Path::new("index.json")));
    }
}

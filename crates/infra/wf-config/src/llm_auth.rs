//! LLM authentication declaration loading.
//!
//! Auth declarations are stored as a family directory (e.g. `llm-auth/`)
//! containing an `index.json` plus one `*.toml` per provider. This module
//! aggregates those files into an [`AuthConfig`] so `LlmConfig.auth` can be
//! populated from user-level configuration instead of programmatic defaults.

use std::path::{Path, PathBuf};

use wf_types::config::llm_auth::{AuthConfig, AuthProviderConfig};

use crate::error::ConfigResult;
use crate::index::{expand_index_paths, load_index_file};
use crate::layout;
use crate::processor::llm_auth::{transform_auth_providers, validate_auth_config};

/// Candidate roots for the `llm-auth` family, in precedence order (highest
/// first). Only explicitly provided sources are considered: the project
/// override and the caller-provided settings directory. Ambient
/// home-directory state joins only when the caller opts in; services leave
/// ambient disabled so process environment can never inject providers.
/// The in-repository workstation demo is documentation only and is never
/// consulted here.
/// environment can never inject providers.
pub fn candidate_auth_dirs(
    project_root: &Path,
    settings_dir: Option<&Path>,
    include_ambient: bool,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // Project-level override (tracked config).
    dirs.push(
        project_root
            .join(layout::PROJECT_WF_DIR)
            .join(layout::family::LLM_AUTH),
    );
    // Explicitly provided global settings directory.
    if let Some(settings) = settings_dir {
        dirs.push(settings.join(layout::family::LLM_AUTH));
    }
    if !include_ambient {
        return dirs;
    }
    // Ambient workstation state: the home-directory user root joins only
    // on explicit opt-in.
    if let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
    {
        dirs.push(
            PathBuf::from(home)
                .join(".wf")
                .join(layout::family::LLM_AUTH),
        );
    }
    dirs
}

/// Load a single auth provider declaration from a file.
pub fn load_auth_provider(path: &Path) -> ConfigResult<AuthProviderConfig> {
    let provider: AuthProviderConfig = crate::parser::parse_config_file(path)?;
    crate::processor::llm_auth::validate_auth_provider(&provider)?;
    let transformed = transform_auth_providers(
        std::slice::from_ref(&provider),
        &std::collections::HashMap::new(),
    )?;
    Ok(transformed.into_iter().next().unwrap_or(provider))
}

/// Load an [`AuthConfig`] from a family directory. Returns `None` when the
/// directory does not exist or yields no providers.
pub fn load_auth_config_from_dir(dir: &Path) -> ConfigResult<Option<AuthConfig>> {
    if !dir.exists() {
        return Ok(None);
    }

    let mut providers = Vec::new();
    let index_path = dir.join(crate::layout::INDEX_FILE_NAME);
    if index_path.exists() {
        let index = load_index_file(dir)?;
        let file_paths = expand_index_paths(&index, dir);
        for file_path in file_paths {
            // Skip the index file itself.
            let path = Path::new(&file_path);
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case(crate::layout::INDEX_FILE_NAME))
                .unwrap_or(false)
            {
                continue;
            }
            match load_auth_provider(path) {
                Ok(provider) => providers.push(provider),
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "skipping llm-auth declaration")
                }
            }
        }
    } else {
        // Fall back to a flat glob of *.toml / *.json in the directory.
        // Documentation samples never match (see `is_example_path`).
        let pattern = dir.join("*.toml");
        if let Ok(paths) = crate::loader::expand_glob_paths(&pattern.to_string_lossy()) {
            for path in paths {
                if crate::layout::is_example_path(&path) {
                    continue;
                }
                if let Ok(provider) = load_auth_provider(&path) {
                    providers.push(provider);
                }
            }
        }
        let pattern = dir.join("*.json");
        if let Ok(paths) = crate::loader::expand_glob_paths(&pattern.to_string_lossy()) {
            for path in paths {
                if crate::layout::is_example_path(&path) {
                    continue;
                }
                if let Ok(provider) = load_auth_provider(&path) {
                    providers.push(provider);
                }
            }
        }
    }

    if providers.is_empty() {
        return Ok(None);
    }
    validate_auth_config(&providers)?;
    Ok(Some(AuthConfig {
        store_path: None,
        providers,
    }))
}

/// Load an [`AuthConfig`] from the first candidate directory that yields
/// providers. Returns the default (empty) config when none is found.
/// Ambient home-directory sources are only consulted when the caller opts
/// in; services keep them disabled.
pub fn load_llm_auth_config(
    project_root: &Path,
    settings_dir: Option<&Path>,
    include_ambient: bool,
) -> ConfigResult<AuthConfig> {
    for dir in candidate_auth_dirs(project_root, settings_dir, include_ambient) {
        if let Some(auth) = load_auth_config_from_dir(&dir)? {
            return Ok(auth);
        }
    }
    Ok(AuthConfig::default())
}

/// Project-level path helper kept for compatibility with the skill/mcp
/// loaders: `{project_root}/.wf/llm-auth` (highest tracked override).
pub fn get_project_llm_auth_path(project_root: &Path) -> PathBuf {
    project_root
        .join(layout::PROJECT_WF_DIR)
        .join(layout::family::LLM_AUTH)
}

/// Global path helper: `{settings_dir}/llm-auth` when a settings dir is
/// known.
pub fn get_global_llm_auth_path(settings_dir: &Path) -> PathBuf {
    settings_dir.join(layout::family::LLM_AUTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_disabled_yields_explicit_sources_only() {
        let project = std::path::Path::new("/repo");
        let settings = std::path::Path::new("/global");
        let dirs = candidate_auth_dirs(project, Some(settings), false);
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0], get_project_llm_auth_path(project));
        assert_eq!(dirs[1], get_global_llm_auth_path(settings));
    }

    #[test]
    fn ambient_disabled_without_settings_dir_is_project_only() {
        let project = std::path::Path::new("/repo");
        let dirs = candidate_auth_dirs(project, None, false);
        assert_eq!(dirs, vec![get_project_llm_auth_path(project)]);
    }

    #[test]
    fn example_declarations_never_load() {
        let dir = tempfile::tempdir().unwrap();
        let family = dir.path().join("llm-auth");
        std::fs::create_dir_all(&family).unwrap();
        std::fs::write(
            family.join("index.json"),
            r#"{"version": "1.0", "type": "llm_auth", "paths": ["./*.toml"]}"#,
        )
        .unwrap();
        std::fs::write(
            family.join("demo.auth.example.toml"),
            "id = \"demo\"\nmethod = \"api_key\"\n",
        )
        .unwrap();
        assert!(load_auth_config_from_dir(&family).unwrap().is_none());
    }
}

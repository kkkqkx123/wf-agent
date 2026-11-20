//! LLM authentication declaration loading.
//!
//! Auth declarations are stored as a family directory (e.g. `llm-auth/`)
//! containing an `index.json` plus one `*.toml` per provider. This module
//! aggregates those files into an [`AuthConfig`] so `LlmConfig.auth` can be
//! populated from user-level configuration instead of programmatic defaults.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wf_types::config::llm_auth::{AuthConfig, AuthProviderConfig};

use crate::error::ConfigResult;
use crate::index::{expand_index_paths, load_index_file};
use crate::layout;
use crate::processor::llm_auth::{transform_auth_providers, validate_auth_config};

/// Candidate roots for the `llm-auth` family, lowest precedence first:
/// ambient workstation state, explicit settings directory, project override.
/// Ambient joins only when the caller opts in; services leave it disabled.
pub fn candidate_auth_dirs(
    project_root: Option<&Path>,
    settings_dir: Option<&Path>,
    include_ambient: bool,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if include_ambient {
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
    }
    if let Some(settings) = settings_dir {
        dirs.push(settings.join(layout::family::LLM_AUTH));
    }
    if let Some(project) = project_root {
        dirs.push(
            project
                .join(layout::PROJECT_WF_DIR)
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
/// directory does not exist or yields no providers. Every declared file must
/// be valid; a bad declaration aborts the load.
pub fn load_auth_config_from_dir(dir: &Path) -> ConfigResult<Option<AuthConfig>> {
    if !dir.exists() {
        return Ok(None);
    }

    let mut providers = Vec::new();
    let index_path = dir.join(crate::layout::INDEX_FILE_NAME);
    if index_path.exists() {
        let index = load_index_file(dir)?;
        let file_paths = expand_index_paths(&index, dir)?;
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
            let provider = load_auth_provider(path).map_err(|e| {
                crate::error::ConfigError::Validation(format!(
                    "invalid llm-auth declaration {}: {e}",
                    path.display()
                ))
            })?;
            providers.push(provider);
        }
    } else {
        // Fall back to a flat glob of *.toml / *.json in the directory.
        // Documentation samples never match (see `is_example_path`).
        for ext in ["toml", "json"] {
            let pattern = dir.join(format!("*.{ext}"));
            let paths = crate::loader::expand_glob_paths(&pattern.to_string_lossy())?;
            for path in paths {
                if crate::layout::is_example_path(&path) {
                    continue;
                }
                let provider = load_auth_provider(&path).map_err(|e| {
                    crate::error::ConfigError::Validation(format!(
                        "invalid llm-auth declaration {}: {e}",
                        path.display()
                    ))
                })?;
                providers.push(provider);
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

/// Merge auth declarations from every candidate directory by provider id.
/// Later directories override earlier ones, so the project layer wins.
pub fn load_llm_auth_config(
    project_root: Option<&Path>,
    settings_dir: Option<&Path>,
    include_ambient: bool,
) -> ConfigResult<AuthConfig> {
    let mut merged = AuthConfig::default();
    for dir in candidate_auth_dirs(project_root, settings_dir, include_ambient) {
        if let Some(auth) = load_auth_config_from_dir(&dir)? {
            merged = merge_auth_configs(merged, auth);
        }
    }
    Ok(merged)
}

/// Merge two auth configurations by provider id. Values in `overlay` win.
pub fn merge_auth_configs(base: AuthConfig, overlay: AuthConfig) -> AuthConfig {
    let mut providers: BTreeMap<String, AuthProviderConfig> = base
        .providers
        .into_iter()
        .map(|provider| (provider.id.clone(), provider))
        .collect();
    for provider in overlay.providers {
        providers.insert(provider.id.clone(), provider);
    }
    AuthConfig {
        store_path: overlay.store_path.or(base.store_path),
        providers: providers.into_values().collect(),
    }
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
        let dirs = candidate_auth_dirs(Some(project), Some(settings), false);
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0], get_global_llm_auth_path(settings));
        assert_eq!(dirs[1], get_project_llm_auth_path(project));
    }

    #[test]
    fn ambient_disabled_without_settings_dir_is_project_only() {
        let project = std::path::Path::new("/repo");
        let dirs = candidate_auth_dirs(Some(project), None, false);
        assert_eq!(dirs, vec![get_project_llm_auth_path(project)]);
    }

    #[test]
    fn auth_records_merge_by_id_with_overlay_priority() {
        use wf_types::config::llm_auth::AuthMethodConfig;

        let provider = |id: &str, display: &str| AuthProviderConfig {
            id: id.to_string(),
            method: AuthMethodConfig::ApiKey,
            display_name: display.to_string(),
            login_label: None,
            env: Some("KEY".to_string()),
            flow: None,
            is_subscription: None,
            device_authorization_url: None,
            authorize_url: None,
            token_url: None,
            client_id: None,
            scope: None,
            redirect_uri: None,
            loopback: None,
            token_content_type: None,
            preset: None,
        };
        let base = AuthConfig {
            store_path: Some("base.json".to_string()),
            providers: vec![provider("a", "base-a"), provider("b", "base-b")],
        };
        let overlay = AuthConfig {
            store_path: None,
            providers: vec![provider("a", "overlay-a")],
        };

        let merged = merge_auth_configs(base, overlay);
        assert_eq!(merged.store_path.as_deref(), Some("base.json"));
        assert_eq!(merged.providers.len(), 2);
        let a = merged.providers.iter().find(|p| p.id == "a").unwrap();
        assert_eq!(a.display_name, "overlay-a");
    }

    #[test]
    fn invalid_declaration_fails_fast() {
        let dir = tempfile::tempdir().unwrap();
        let family = dir.path().join("llm-auth");
        std::fs::create_dir_all(&family).unwrap();
        std::fs::write(
            family.join("bad.auth.toml"),
            "id = \"bad\"\nmethod = \"unknown\"\ndisplay_name = \"Bad\"\n",
        )
        .unwrap();
        assert!(load_auth_config_from_dir(&family).is_err());
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

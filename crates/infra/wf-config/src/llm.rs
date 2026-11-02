//! LLM profile and provider-definition file families.
//!
//! Profiles and provider definitions are stored as one file per record
//! under `llm-profiles/` and `llm-providers/`. Each family directory
//! supports an `index.json` manifest or a flat `*.toml` / `*.json` glob,
//! mirroring the `llm-auth` family. The settings layer is low priority,
//! the project `.wf/` layer wins by record `id`.
//!
//! Error policy has two tiers: an unreadable or unparsable file is skipped
//! with a warning so one bad record cannot hide every other profile,
//! while a parsed record that fails validation fails the whole load so
//! a typoed profile cannot silently run with defaults.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult};
use crate::index::{expand_index_paths, load_index_file};

/// Candidate roots for the `llm-profiles` family, lowest precedence first:
/// ambient workstation state, explicit settings directory, project override.
/// Ambient joins only on explicit opt-in; services keep it disabled.
pub fn candidate_llm_profile_dirs(
    project_root: &Path,
    settings_dir: Option<&Path>,
    include_ambient: bool,
) -> Vec<PathBuf> {
    candidate_family_dirs(
        project_root,
        settings_dir,
        crate::layout::family::LLM_PROFILES,
        include_ambient,
    )
}

/// Candidate roots for the `llm-providers` family, lowest precedence first.
pub fn candidate_llm_provider_dirs(
    project_root: &Path,
    settings_dir: Option<&Path>,
    include_ambient: bool,
) -> Vec<PathBuf> {
    candidate_family_dirs(
        project_root,
        settings_dir,
        crate::layout::family::LLM_PROVIDERS,
        include_ambient,
    )
}

fn candidate_family_dirs(
    project_root: &Path,
    settings_dir: Option<&Path>,
    family: &str,
    include_ambient: bool,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if include_ambient {
        if let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .filter(|value| !value.is_empty())
        {
            dirs.push(PathBuf::from(home).join(".wf").join(family));
        }
    }
    if let Some(settings) = settings_dir {
        dirs.push(settings.join(family));
    }
    if !project_root.as_os_str().is_empty() {
        dirs.push(
            project_root
                .join(crate::layout::PROJECT_WF_DIR)
                .join(family),
        );
    }
    dirs
}

/// Project-layer path helper: `{project_root}/.wf/llm-profiles`.
pub fn get_project_llm_profiles_path(project_root: &Path) -> PathBuf {
    project_root
        .join(crate::layout::PROJECT_WF_DIR)
        .join(crate::layout::family::LLM_PROFILES)
}

/// Global path helper: `{settings_dir}/llm-profiles`.
pub fn get_global_llm_profiles_path(settings_dir: &Path) -> PathBuf {
    settings_dir.join(crate::layout::family::LLM_PROFILES)
}

/// Project-layer path helper: `{project_root}/.wf/llm-providers`.
pub fn get_project_llm_provider_path(project_root: &Path) -> PathBuf {
    project_root
        .join(crate::layout::PROJECT_WF_DIR)
        .join(crate::layout::family::LLM_PROVIDERS)
}

/// Global path helper: `{settings_dir}/llm-providers`.
pub fn get_global_llm_provider_path(settings_dir: &Path) -> PathBuf {
    settings_dir.join(crate::layout::family::LLM_PROVIDERS)
}

/// Load one profile file, applying env interpolation before parsing.
pub fn load_llm_profile_file(path: &Path) -> ConfigResult<wf_types::llm::LlmProfile> {
    let profile: wf_types::llm::LlmProfile = crate::parser::parse_config_file(path)?;
    crate::processor::llm_profile::validate_llm_profile(&profile)?;
    crate::processor::llm_profile::transform_llm_profile(
        &profile,
        &std::collections::HashMap::new(),
    )
}

/// Load one provider-definition file.
pub fn load_llm_provider_file(path: &Path) -> ConfigResult<wf_types::llm::LlmProviderDefinition> {
    let definition: wf_types::llm::LlmProviderDefinition = crate::parser::parse_config_file(path)?;
    crate::processor::llm_profile::validate_provider_definition(&definition)?;
    Ok(definition)
}

/// List candidate files inside one family directory: `index.json` wins when
/// present, otherwise a flat `*.toml` / `*.json` glob. Sample files
/// (`*.example.*`) never match.
fn family_candidate_files(dir: &Path) -> Vec<PathBuf> {
    let index_path = dir.join(crate::layout::INDEX_FILE_NAME);
    if index_path.exists() {
        match load_index_file(dir) {
            Ok(index) => expand_index_paths(&index, dir)
                .into_iter()
                .map(PathBuf::from)
                .filter(|path| !crate::layout::is_example_path(path))
                .filter(|path| {
                    !path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.eq_ignore_ascii_case(crate::layout::INDEX_FILE_NAME))
                        .unwrap_or(false)
                })
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, path = %dir.display(), "skipping invalid llm index");
                Vec::new()
            }
        }
    } else {
        let mut out = Vec::new();
        for ext in ["toml", "json"] {
            let pattern = dir.join(format!("*.{ext}"));
            if let Ok(paths) = crate::loader::expand_glob_paths(&pattern.to_string_lossy()) {
                out.extend(
                    paths
                        .into_iter()
                        .filter(|p| !crate::layout::is_example_path(p)),
                );
            }
        }
        out.sort();
        out
    }
}

/// Reject files whose raw content leaves `${VAR}` placeholders without a
/// value or a declared default. Called only in strict mode.
fn ensure_strict_env(path: &Path, strict_env: bool) -> ConfigResult<()> {
    if !strict_env {
        return Ok(());
    }
    let content = std::fs::read_to_string(path)?;
    crate::env::ensure_no_unresolved_env_vars(&content)
        .map_err(|e| ConfigError::EnvVar(format!("{} in {}", e, path.display())))
}

fn load_family_records<T: Clone>(
    dirs: &[PathBuf],
    strict_env: bool,
    load_one: impl Fn(&Path) -> ConfigResult<T>,
    record_id: impl Fn(&T) -> &str,
) -> ConfigResult<Vec<T>> {
    let mut merged: BTreeMap<String, T> = BTreeMap::new();
    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        for path in family_candidate_files(dir) {
            ensure_strict_env(&path, strict_env)?;
            match load_one(&path) {
                Ok(record) => {
                    merged.insert(record_id(&record).to_string(), record);
                }
                Err(ConfigError::Validation(message)) => {
                    return Err(ConfigError::Validation(format!(
                        "invalid llm config {}: {message}",
                        path.display()
                    )));
                }
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "skipping llm config file");
                }
            }
        }
    }
    Ok(merged.into_values().collect())
}

/// Load merged profiles from settings (low) and project (high) layers.
/// Returns an empty vector when no family directory yields records.
pub fn load_llm_profiles_config(
    project_root: &Path,
    settings_dir: Option<&Path>,
    include_ambient: bool,
    strict_env: bool,
) -> ConfigResult<Vec<wf_types::llm::LlmProfile>> {
    let dirs = candidate_llm_profile_dirs(project_root, settings_dir, include_ambient);
    load_family_records(&dirs, strict_env, load_llm_profile_file, |p| &p.id)
}

/// Load merged provider definitions from settings (low) and project layers.
pub fn load_llm_provider_definitions_config(
    project_root: &Path,
    settings_dir: Option<&Path>,
    include_ambient: bool,
    strict_env: bool,
) -> ConfigResult<Vec<wf_types::llm::LlmProviderDefinition>> {
    let dirs = candidate_llm_provider_dirs(project_root, settings_dir, include_ambient);
    load_family_records(&dirs, strict_env, load_llm_provider_file, |p| &p.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn profile_toml(id: &str, model: &str) -> String {
        format!("id = \"{id}\"\nname = \"{id}\"\nformat = \"openai_chat\"\nmodel = \"{model}\"\n")
    }

    #[test]
    fn project_profile_wins_over_settings() {
        let root = tempfile::tempdir().unwrap();
        let settings = root.path().join("settings");
        let project = root.path().join("proj");
        write(
            &settings.join("llm-profiles").join("a.toml"),
            &profile_toml("a", "settings-model"),
        );
        write(
            &settings.join("llm-profiles").join("b.toml"),
            &profile_toml("b", "settings-b"),
        );
        write(
            &project.join(".wf").join("llm-profiles").join("a.toml"),
            &profile_toml("a", "project-model"),
        );

        let profiles =
            load_llm_profiles_config(&project, Some(settings.as_path()), false, false).unwrap();
        assert_eq!(profiles.len(), 2);
        let by_id: BTreeMap<_, _> = profiles.into_iter().map(|p| (p.id.clone(), p)).collect();
        assert_eq!(by_id["a"].model, "project-model");
        assert_eq!(by_id["b"].model, "settings-b");
    }

    #[test]
    fn example_files_never_load() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("proj");
        write(
            &project
                .join(".wf")
                .join("llm-profiles")
                .join("demo.profile.example.toml"),
            &profile_toml("demo", "m"),
        );
        let profiles = load_llm_profiles_config(&project, None, false, false).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn unparsable_file_is_skipped_with_warning() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("proj");
        write(
            &project.join(".wf").join("llm-profiles").join("bad.toml"),
            "not valid toml [[[",
        );
        write(
            &project.join(".wf").join("llm-profiles").join("good.toml"),
            &profile_toml("good", "m"),
        );
        let profiles = load_llm_profiles_config(&project, None, false, false).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, "good");
    }

    #[test]
    fn invalid_record_fails_fast() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("proj");
        write(
            &project.join(".wf").join("llm-profiles").join("bad.toml"),
            "id = \"\"\nname = \"x\"\nformat = \"openai_chat\"\nmodel = \"m\"\n",
        );
        assert!(load_llm_profiles_config(&project, None, false, false).is_err());
    }

    #[test]
    fn strict_mode_rejects_unresolved_placeholders() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("proj");
        std::env::remove_var("WF_TEST_LLM_STRICT_MISSING");
        write(
            &project.join(".wf").join("llm-profiles").join("a.toml"),
            "id = \"a\"\nname = \"a\"\nformat = \"openai_chat\"\nmodel = \"m\"\napi_key = \"${WF_TEST_LLM_STRICT_MISSING}\"\n",
        );
        assert!(load_llm_profiles_config(&project, None, false, false).is_ok());
        assert!(load_llm_profiles_config(&project, None, false, true).is_err());
    }

    #[test]
    fn providers_merge_by_id() {
        let root = tempfile::tempdir().unwrap();
        let settings = root.path().join("settings");
        let project = root.path().join("proj");
        write(
            &settings.join("llm-providers").join("p.toml"),
            "id = \"p\"\nformat = \"openai_chat\"\nbase_url = \"https://settings.test\"\n",
        );
        write(
            &project.join(".wf").join("llm-providers").join("p.toml"),
            "id = \"p\"\nformat = \"openai_chat\"\nbase_url = \"https://project.test\"\n",
        );
        let providers =
            load_llm_provider_definitions_config(&project, Some(settings.as_path()), false, false)
                .unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(
            providers[0].base_url.as_deref(),
            Some("https://project.test")
        );
    }
}

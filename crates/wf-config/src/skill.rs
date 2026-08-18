//! Skill settings loading and merging.
//!
//! Settings are merged from the global settings directory and project-level
//! `.wf/skills.json` / `.agent/skills.json` files, with project files taking
//! priority over the global file. Collection mode (via the shared preset
//! loader) resolves skill collections from `configs/skills/`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult};
use wf_types::config::SkillCollectionFile;
use wf_types::skill::SkillConfig;

pub const DEFAULT_SKILL_SETTINGS_FILE: &str = "skill-settings.json";
pub const PROJECT_SKILL_FILE: &str = ".agent/skills.json";
pub const PROJECT_WF_SKILL_FILE: &str = ".wf/skills.json";

/// Default empty skill config.
pub fn create_default_skill_config() -> SkillConfig {
    SkillConfig {
        paths: Vec::new(),
        auto_scan: Some(true),
    }
}

/// Global settings file: `{settings_dir}/skill-settings.json`.
pub fn get_global_skill_settings_path(settings_dir: &Path) -> PathBuf {
    settings_dir.join(DEFAULT_SKILL_SETTINGS_FILE)
}

/// Project-specific file: `{project_root}/.agent/skills.json`.
pub fn get_project_skill_path(project_root: &Path) -> PathBuf {
    project_root.join(PROJECT_SKILL_FILE)
}

/// Project-specific file: `{project_root}/.wf/skills.json` (highest priority).
pub fn get_project_wf_skill_path(project_root: &Path) -> PathBuf {
    project_root.join(PROJECT_WF_SKILL_FILE)
}

/// Project settings files in priority order (highest first).
pub fn get_project_skill_paths(project_root: &Path) -> Vec<PathBuf> {
    vec![
        get_project_wf_skill_path(project_root),
        get_project_skill_path(project_root),
    ]
}

/// Load a single skill settings file.
///
/// Returns `None` when the file does not exist. A malformed JSON body or an
/// invalid `paths` / `autoScan` value is reported as an error.
pub fn load_skill_config(file_path: &Path) -> ConfigResult<Option<SkillConfig>> {
    if !file_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(file_path)?;
    let value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        ConfigError::Parse(format!(
            "Failed to parse skill settings file: {}: {e}",
            file_path.display()
        ))
    })?;
    normalize_skill_config(&value, file_path).map(Some)
}

/// Normalize and validate an unknown value into a `SkillConfig`.
fn normalize_skill_config(
    value: &serde_json::Value,
    file_path: &Path,
) -> ConfigResult<SkillConfig> {
    let obj = value.as_object().ok_or_else(|| {
        ConfigError::Validation(format!(
            "Invalid skill settings in {}: expected a JSON object",
            file_path.display()
        ))
    })?;

    let mut paths = Vec::new();
    if let Some(raw_paths) = obj.get("paths") {
        let arr = raw_paths.as_array().ok_or_else(|| {
            ConfigError::Validation(format!(
                "Invalid skill settings in {}: 'paths' must be an array of strings",
                file_path.display()
            ))
        })?;
        for item in arr {
            let p = item.as_str().ok_or_else(|| {
                ConfigError::Validation(format!(
                    "Invalid skill settings in {}: 'paths' must be an array of strings",
                    file_path.display()
                ))
            })?;
            paths.push(p.to_string());
        }
    }

    let mut auto_scan = None;
    if let Some(raw_scan) = obj.get("autoScan") {
        let b = raw_scan.as_bool().ok_or_else(|| {
            ConfigError::Validation(format!(
                "Invalid skill settings in {}: 'autoScan' must be a boolean",
                file_path.display()
            ))
        })?;
        auto_scan = Some(b);
    }

    Ok(SkillConfig { paths, auto_scan })
}

/// Merge global, `.wf`, and `.agent` skill configs into a single config.
///
/// Merge rules:
/// - **paths**: union, deduplicated, `.wf` first then `.agent` then global.
/// - **auto_scan**: `.wf` wins if provided, then `.agent`, then global,
///   then the default (`true`).
pub fn merge_skill_configs(
    global_config: Option<&SkillConfig>,
    wf_config: Option<&SkillConfig>,
    agent_config: Option<&SkillConfig>,
) -> SkillConfig {
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged_paths: Vec<String> = Vec::new();

    for config in [wf_config, agent_config, global_config]
        .into_iter()
        .flatten()
    {
        for p in &config.paths {
            if seen.insert(p.clone()) {
                merged_paths.push(p.clone());
            }
        }
    }

    let auto_scan = wf_config
        .and_then(|c| c.auto_scan)
        .or_else(|| agent_config.and_then(|c| c.auto_scan))
        .or_else(|| global_config.and_then(|c| c.auto_scan))
        .or(Some(true));

    SkillConfig {
        paths: merged_paths,
        auto_scan,
    }
}

/// Load and merge skill settings from the global directory and all project
/// files. Priority chain (highest first): `.wf/skills.json` > `.agent/skills.json`
/// > global `skill-settings.json`. Missing files are skipped.
pub fn load_and_merge_skill_config(
    settings_dir: &Path,
    project_root: &Path,
) -> ConfigResult<SkillConfig> {
    let global_path = get_global_skill_settings_path(settings_dir);
    let project_paths = get_project_skill_paths(project_root);

    let global_config = load_skill_config(&global_path)?;
    let wf_config = load_skill_config(project_paths[0].as_path())?;
    let agent_config = load_skill_config(project_paths[1].as_path())?;

    Ok(merge_skill_configs(
        global_config.as_ref(),
        wf_config.as_ref(),
        agent_config.as_ref(),
    ))
}

/// Write skill config to a JSON file (camelCase `autoScan`, matching TS).
pub fn write_skill_config(file_path: &Path, config: &SkillConfig) -> ConfigResult<()> {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "paths".to_string(),
        serde_json::Value::Array(
            config
                .paths
                .iter()
                .map(|p| serde_json::Value::String(p.clone()))
                .collect(),
        ),
    );
    obj.insert(
        "autoScan".to_string(),
        serde_json::Value::Bool(config.auto_scan.unwrap_or(true)),
    );
    let content = serde_json::to_string_pretty(&serde_json::Value::Object(obj))?;
    std::fs::write(file_path, content)?;
    Ok(())
}

/// Ensure a skill settings file exists, creating a default one if absent.
/// Returns `true` if the file was created, `false` if it already existed.
pub fn ensure_skill_config_file(file_path: &Path) -> ConfigResult<bool> {
    if file_path.exists() {
        return Ok(false);
    }
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_skill_config(file_path, &create_default_skill_config())?;
    Ok(true)
}

/// Default skill preset directory: `{project_root}/configs/skills`.
pub fn get_default_skill_preset_dir(project_root: &Path) -> PathBuf {
    project_root.join("configs").join("skills")
}

/// Load a skill collection definition by name (collection mode).
///
/// Resolution: load `configs/skills/index.json` via the shared preset loader,
/// match `collection_name` to a collection file by filename, then parse it.
pub fn load_skill_collection(
    base_dir: &Path,
    collection_name: &str,
) -> ConfigResult<SkillCollectionFile> {
    let resolved = crate::preset::resolve_preset_index(base_dir)?;
    let entry =
        crate::preset::find_preset_by_name(&resolved, collection_name).ok_or_else(|| {
            let available = crate::preset::list_preset_names(&resolved).join(", ");
            ConfigError::NotFound(format!(
                "Skill collection \"{collection_name}\" not found in {}. Available collections: {}",
                base_dir.display(),
                if available.is_empty() {
                    "(none)".to_string()
                } else {
                    available
                }
            ))
        })?;
    crate::preset::load_single_file_preset::<SkillCollectionFile>(entry)
}

/// Expand a skill collection's `paths` into resolved skill file paths
/// (glob patterns are resolved relative to `base_dir`, then deduplicated).
///
/// A trailing `/**` (as commonly written in collection files) is normalized
/// to `/**/*` because the `glob` crate requires a separator after `**` to
/// match children.
pub fn expand_skill_collection_paths(
    collection: &SkillCollectionFile,
    base_dir: &Path,
) -> Vec<PathBuf> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut all_paths: Vec<PathBuf> = Vec::new();

    for pattern in &collection.paths {
        let normalized = pattern.trim_end_matches('/').to_string();
        let normalized = if normalized.ends_with("/**") {
            format!("{normalized}/*")
        } else {
            normalized
        };
        let full = base_dir.join(&normalized);
        let full_str = full.to_string_lossy().to_string();
        let matches: Vec<PathBuf> = if full_str.contains('*') {
            crate::loader::expand_glob_paths(&full_str).unwrap_or_default()
        } else if full.exists() {
            vec![full]
        } else {
            Vec::new()
        };
        for m in matches {
            if seen.insert(m.clone()) {
                all_paths.push(m);
            }
        }
    }
    all_paths
}

/// Load skill config with collection support.
///
/// Tries collection mode first (when `configs/skills/index.json` exists and a
/// collection name is provided), then falls back to the legacy
/// global/project config chain. Collection paths are merged ahead of legacy
/// paths.
pub fn load_and_merge_skill_config_with_collection(
    settings_dir: &Path,
    project_root: &Path,
    collection_name: Option<&str>,
) -> ConfigResult<SkillConfig> {
    let preset_dir = get_default_skill_preset_dir(project_root);
    let index_path = preset_dir.join(crate::preset::INDEX_FILE_NAME);

    if !index_path.exists() {
        return load_and_merge_skill_config(settings_dir, project_root);
    }

    let collection_paths: Vec<PathBuf> = match collection_name {
        Some(name) => {
            let collection = match load_skill_collection(&preset_dir, name) {
                Ok(c) => c,
                Err(_) => return load_and_merge_skill_config(settings_dir, project_root),
            };
            expand_skill_collection_paths(&collection, &preset_dir)
        }
        None => Vec::new(),
    };

    let legacy = load_and_merge_skill_config(settings_dir, project_root)?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut merged_paths: Vec<String> = Vec::new();
    for p in collection_paths {
        let p = p.to_string_lossy().to_string();
        if seen.insert(p.clone()) {
            merged_paths.push(p);
        }
    }
    for p in legacy.paths {
        if seen.insert(p.clone()) {
            merged_paths.push(p);
        }
    }

    Ok(SkillConfig {
        paths: merged_paths,
        auto_scan: legacy.auto_scan,
    })
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
    fn test_merge_priority_and_dedup() {
        let global = SkillConfig {
            paths: vec!["g1".into(), "g2".into(), "shared".into()],
            auto_scan: Some(false),
        };
        let wf = SkillConfig {
            paths: vec!["w1".into(), "shared".into()],
            auto_scan: Some(true),
        };
        let agent = SkillConfig {
            paths: vec!["a1".into(), "w1".into()],
            auto_scan: Some(false),
        };

        let merged = merge_skill_configs(Some(&global), Some(&wf), Some(&agent));
        assert_eq!(merged.paths, vec!["w1", "shared", "a1", "g1", "g2"]);
        assert_eq!(merged.auto_scan, Some(true));
    }

    #[test]
    fn test_merge_auto_scan_fallbacks() {
        let global = SkillConfig {
            paths: vec![],
            auto_scan: Some(false),
        };
        // wf/agent absent → global wins.
        assert_eq!(
            merge_skill_configs(Some(&global), None, None).auto_scan,
            Some(false)
        );
        // nothing set → default true.
        assert_eq!(merge_skill_configs(None, None, None).auto_scan, Some(true));
        // agent wins over global.
        let agent = SkillConfig {
            paths: vec![],
            auto_scan: Some(true),
        };
        assert_eq!(
            merge_skill_configs(Some(&global), None, Some(&agent)).auto_scan,
            Some(true)
        );
    }

    #[test]
    fn test_missing_file_returns_none() {
        let dir = std::env::temp_dir().join(format!("wf-skill-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load_skill_config(&dir.join("missing.json"))
            .unwrap()
            .is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_and_merge_skill_config() {
        let root = std::env::temp_dir().join(format!("wf-skill-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");

        write_json(
            &root.join("skill-settings.json"),
            r#"{"paths": ["g1", "shared"], "autoScan": false}"#,
        );
        write_json(&project.join(".agent/skills.json"), r#"{"paths": ["a1"]}"#);
        write_json(
            &project.join(".wf/skills.json"),
            r#"{"paths": ["w1", "shared"], "autoScan": true}"#,
        );

        let merged = load_and_merge_skill_config(&root, &project).unwrap();
        assert_eq!(merged.paths, vec!["w1", "shared", "a1", "g1"]);
        assert_eq!(merged.auto_scan, Some(true));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_invalid_json_reports_error() {
        let dir = std::env::temp_dir().join(format!("wf-skill-invalid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write_json(&dir.join("bad.json"), "{not json");
        assert!(load_skill_config(&dir.join("bad.json")).is_err());
        write_json(
            &dir.join("bad-scan.json"),
            r#"{"paths": [], "autoScan": "yes"}"#,
        );
        assert!(load_skill_config(&dir.join("bad-scan.json")).is_err());
        write_json(&dir.join("bad-paths.json"), r#"{"paths": [1, 2]}"#);
        assert!(load_skill_config(&dir.join("bad-paths.json")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ensure_and_write_skill_config() {
        let dir = std::env::temp_dir().join(format!("wf-skill-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("skill-settings.json");

        assert!(ensure_skill_config_file(&path).unwrap());
        let loaded = load_skill_config(&path).unwrap().unwrap();
        assert_eq!(loaded.auto_scan, Some(true));
        // Second call does not recreate.
        assert!(!ensure_skill_config_file(&path).unwrap());

        write_skill_config(
            &path,
            &SkillConfig {
                paths: vec!["/skills".into()],
                auto_scan: Some(false),
            },
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"autoScan\": false"));
        let loaded = load_skill_config(&path).unwrap().unwrap();
        assert_eq!(loaded.paths, vec!["/skills"]);
        assert_eq!(loaded.auto_scan, Some(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_skill_collection_loading() {
        let root = std::env::temp_dir().join(format!("wf-skill-col-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");
        let skills_dir = project.join("configs").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        write_json(
            &skills_dir.join("index.json"),
            r#"{"version": "1.0", "type": "skill_presets", "paths": ["./*.json"]}"#,
        );
        write_json(
            &skills_dir.join("default.json"),
            r#"{"id": "default", "name": "Default", "paths": ["./skills/a/**"]}"#,
        );
        std::fs::create_dir_all(skills_dir.join("skills").join("a")).unwrap();
        write_json(
            &skills_dir.join("skills").join("a").join("one.json"),
            r#"{"id": "one"}"#,
        );

        let collection = load_skill_collection(&skills_dir, "default").unwrap();
        assert_eq!(collection.id, "default");
        let paths = expand_skill_collection_paths(&collection, &skills_dir);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].to_string_lossy().ends_with("one.json"));

        assert!(load_skill_collection(&skills_dir, "missing").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_skill_collection_fallback_to_legacy() {
        let root = std::env::temp_dir().join(format!("wf-skill-fallback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");

        // No collection index → legacy chain.
        write_json(
            &root.join("skill-settings.json"),
            r#"{"paths": ["global"], "autoScan": false}"#,
        );
        let merged =
            load_and_merge_skill_config_with_collection(&root, &project, Some("default")).unwrap();
        assert_eq!(merged.paths, vec!["global"]);
        assert_eq!(merged.auto_scan, Some(false));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_skill_collection_merge_order() {
        let root = std::env::temp_dir().join(format!("wf-skill-colmerge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("proj");
        let skills_dir = project.join("configs").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        write_json(
            &skills_dir.join("index.json"),
            r#"{"version": "1.0", "type": "skill_presets", "paths": ["./*.json"]}"#,
        );
        write_json(
            &skills_dir.join("default.json"),
            r#"{"id": "default", "paths": ["./collection-path"]}"#,
        );
        write_json(&skills_dir.join("collection-path"), r#"not a json file"#);
        write_json(
            &root.join("skill-settings.json"),
            r#"{"paths": ["global-path"]}"#,
        );

        let merged =
            load_and_merge_skill_config_with_collection(&root, &project, Some("default")).unwrap();
        assert_eq!(merged.paths.len(), 2);
        assert!(merged.paths[0].ends_with("collection-path"));
        assert_eq!(merged.paths[1], "global-path");
        let _ = std::fs::remove_dir_all(&root);
    }
}

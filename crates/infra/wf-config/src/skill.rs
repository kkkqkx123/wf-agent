//! Skill settings loading and merging.
//!
//! Settings are merged from the global settings directory and the project-level
//! `.wf/skills.json` file, with the project file taking precedence over the
//! global file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult};
use crate::layout;
use wf_types::skill::SkillConfig;

pub const DEFAULT_SKILL_SETTINGS_FILE: &str = "skill-settings.json";

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

/// Project-specific file: `{project_root}/.wf/skills.json` (highest precedence).
pub fn get_project_skill_path(project_root: &Path) -> PathBuf {
    project_root
        .join(layout::PROJECT_WF_DIR)
        .join("skills.json")
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

/// Merge global and project (`.wf`) skill configs into a single config.
///
/// Merge rules:
/// - **paths**: union, deduplicated, `.wf` first then global.
/// - **auto_scan**: `.wf` wins if provided, then global, then the default
///   (`true`).
pub fn merge_skill_configs(
    global_config: Option<&SkillConfig>,
    wf_config: Option<&SkillConfig>,
) -> SkillConfig {
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged_paths: Vec<String> = Vec::new();

    for config in [wf_config, global_config].into_iter().flatten() {
        for p in &config.paths {
            if seen.insert(p.clone()) {
                merged_paths.push(p.clone());
            }
        }
    }

    let auto_scan = wf_config
        .and_then(|c| c.auto_scan)
        .or_else(|| global_config.and_then(|c| c.auto_scan))
        .or(Some(true));

    SkillConfig {
        paths: merged_paths,
        auto_scan,
    }
}

/// Load and merge skill settings from the global directory and the project
/// file. Precedence chain (highest first): `.wf/skills.json` >
/// global `skill-settings.json`. Missing files are skipped.
pub fn load_and_merge_skill_config(
    settings_dir: &Path,
    project_root: &Path,
) -> ConfigResult<SkillConfig> {
    let global_path = get_global_skill_settings_path(settings_dir);
    let project_path = get_project_skill_path(project_root);

    let global_config = load_skill_config(&global_path)?;
    let wf_config = load_skill_config(&project_path)?;

    Ok(merge_skill_configs(
        global_config.as_ref(),
        wf_config.as_ref(),
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
    // Atomic sibling-plus-rename write: a full disk must not truncate the
    // previous settings file.
    wf_common::fs::write_atomic_sync(file_path, content.as_bytes())?;
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
    fn test_merge_precedence_and_dedup() {
        let global = SkillConfig {
            paths: vec!["g1".into(), "g2".into(), "shared".into()],
            auto_scan: Some(false),
        };
        let wf = SkillConfig {
            paths: vec!["w1".into(), "shared".into()],
            auto_scan: Some(true),
        };

        let merged = merge_skill_configs(Some(&global), Some(&wf));
        assert_eq!(merged.paths, vec!["w1", "shared", "g1", "g2"]);
        assert_eq!(merged.auto_scan, Some(true));
    }

    #[test]
    fn test_merge_auto_scan_fallbacks() {
        let global = SkillConfig {
            paths: vec![],
            auto_scan: Some(false),
        };
        // .wf absent → global wins.
        assert_eq!(
            merge_skill_configs(Some(&global), None).auto_scan,
            Some(false)
        );
        // nothing set → default true.
        assert_eq!(merge_skill_configs(None, None).auto_scan, Some(true));
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
        write_json(
            &project.join(".wf/skills.json"),
            r#"{"paths": ["w1", "shared", "a1"], "autoScan": true}"#,
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
}

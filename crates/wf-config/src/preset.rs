//! Generic preset loader.
//!
//! Shared index-based preset resolution skeleton for MCP, Skill and
//! Infrastructure presets:
//!
//! 1. Load `index.json` from the preset directory → get path patterns
//! 2. Expand glob patterns → discover all `*.json` preset files
//! 3. Index presets by filename (without extension) → name → file path map
//! 4. Look up the requested preset name → load the matched file
//!
//! Two preset shapes exist: **single-file presets** (MCP: the preset file
//! IS the config) and **multi-file presets** (Skill/Infrastructure: the file
//! carries `paths` / `files` mappings). `load_single_file_preset` returns the
//! parsed content and the domain layer destructures `files` / `paths` /
//! `mcpServers` itself.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult};

/// Index file name shared by every preset directory.
pub const INDEX_FILE_NAME: &str = "index.json";

/// Default config directories for each preset family.
pub const DEFAULT_CONFIG_DIRS: [&str; 3] =
    ["configs/mcp", "configs/skills", "configs/infrastructure"];

/// Indexed preset entry: maps a preset name to its file path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetEntry {
    /// Preset name (derived from filename without extension).
    pub name: String,
    /// Path to the preset definition file.
    pub file_path: PathBuf,
}

/// A file that failed to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetFailure {
    pub path: String,
    pub error: String,
}

/// Result of resolving a preset index.
#[derive(Debug, Clone, Default)]
pub struct ResolvedPresetIndex {
    /// All discovered presets indexed by name.
    pub presets: HashMap<String, PresetEntry>,
    /// Files that failed to resolve (e.g. duplicate names).
    pub failures: Vec<PresetFailure>,
}

/// Load and resolve a preset index from `{base_dir}/index.json`.
///
/// The index file is a JSON object with a `paths` array of glob patterns
/// (relative to `base_dir`). The index file itself is skipped when a broad
/// pattern like `./*.json` matches it. Duplicate preset names are recorded
/// as failures.
pub fn resolve_preset_index(base_dir: &Path) -> ConfigResult<ResolvedPresetIndex> {
    let index_path = base_dir.join(INDEX_FILE_NAME);
    let content = std::fs::read_to_string(&index_path).map_err(|e| {
        ConfigError::NotFound(format!(
            "Failed to load preset index at {}: {e}",
            index_path.display()
        ))
    })?;
    let value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        ConfigError::Parse(format!(
            "Failed to parse preset index at {}: {e}",
            index_path.display()
        ))
    })?;
    let raw_paths = value
        .get("paths")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ConfigError::Validation(format!(
                "Preset index {} is missing a 'paths' array",
                index_path.display()
            ))
        })?;

    let mut file_paths: Vec<PathBuf> = Vec::new();
    for raw in raw_paths {
        let pattern = raw.as_str().unwrap_or_default();
        let full = base_dir.join(pattern);
        let full_str = full.to_string_lossy().to_string();
        if full_str.contains('*') {
            if let Ok(mut matches) = crate::loader::expand_glob_paths(&full_str) {
                file_paths.append(&mut matches);
            }
        } else {
            file_paths.push(full);
        }
    }

    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut presets: HashMap<String, PresetEntry> = HashMap::new();
    let mut failures: Vec<PresetFailure> = Vec::new();

    for file_path in file_paths {
        // Skip the index file itself when using broad patterns like ./*.json
        let is_index = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case(INDEX_FILE_NAME))
            .unwrap_or(false);
        if is_index || !seen_paths.insert(file_path.clone()) {
            continue;
        }

        let Some(name) = file_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }

        if let Some(existing) = presets.get(name) {
            failures.push(PresetFailure {
                path: file_path.to_string_lossy().to_string(),
                error: format!(
                    "Duplicate preset name \"{name}\" (conflicts with {})",
                    existing.file_path.display()
                ),
            });
            continue;
        }

        presets.insert(
            name.to_string(),
            PresetEntry {
                name: name.to_string(),
                file_path,
            },
        );
    }

    Ok(ResolvedPresetIndex { presets, failures })
}

/// Find a preset entry by name.
pub fn find_preset_by_name<'a>(
    resolved: &'a ResolvedPresetIndex,
    name: &str,
) -> Option<&'a PresetEntry> {
    resolved.presets.get(name)
}

/// List all available preset names (sorted for deterministic output).
pub fn list_preset_names(resolved: &ResolvedPresetIndex) -> Vec<String> {
    let mut names: Vec<String> = resolved.presets.keys().cloned().collect();
    names.sort();
    names
}

/// Load and parse a single-file preset (JSON or TOML, format from extension).
pub fn load_single_file_preset<T: serde::de::DeserializeOwned>(
    entry: &PresetEntry,
) -> ConfigResult<T> {
    crate::parser::parse_config_file(&entry.file_path).map_err(|e| {
        ConfigError::Parse(format!(
            "Failed to parse preset file: {}: {e}",
            entry.file_path.display()
        ))
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

    fn setup_preset_dir(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("wf-preset-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("configs").join("mcp");
        std::fs::create_dir_all(&base).unwrap();
        write_json(
            &base.join("index.json"),
            r#"{"version": "1.0", "type": "mcp_presets", "paths": ["./*.json"]}"#,
        );
        (root, base)
    }

    #[test]
    fn test_resolve_index_expands_and_skips_index() {
        let (root, base) = setup_preset_dir("expand");
        write_json(&base.join("default.json"), r#"{"id": "default"}"#);
        write_json(&base.join("coding.json"), r#"{"id": "coding"}"#);

        let resolved = resolve_preset_index(&base).unwrap();
        assert_eq!(resolved.presets.len(), 2);
        assert!(resolved.presets.contains_key("default"));
        assert!(resolved.presets.contains_key("coding"));
        assert!(!resolved.presets.values().any(|e| e.name == "index"));
        assert!(resolved.failures.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_duplicate_names_recorded_as_failures() {
        let (root, base) = setup_preset_dir("dup");
        write_json(
            &base.join("index.json"),
            r#"{"version": "1.0", "type": "mcp_presets", "paths": ["./*.json", "./*.toml"]}"#,
        );
        write_json(&base.join("default.json"), r#"{"id": "a"}"#);
        write_json(&base.join("default.toml"), r#"id = "b""#);

        let resolved = resolve_preset_index(&base).unwrap();
        assert_eq!(resolved.presets.len(), 1);
        assert_eq!(resolved.failures.len(), 1);
        assert!(resolved.failures[0].error.contains("Duplicate"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_find_list_and_load() {
        let (root, base) = setup_preset_dir("lookup");
        write_json(
            &base.join("default.json"),
            r#"{"id": "default", "servers": {"a": {"type": "stdio", "command": "x"}}}"#,
        );

        let resolved = resolve_preset_index(&base).unwrap();
        assert_eq!(list_preset_names(&resolved), vec!["default"]);

        let entry = find_preset_by_name(&resolved, "default").unwrap();
        #[derive(serde::Deserialize)]
        struct TestPreset {
            id: String,
        }
        let preset: TestPreset = load_single_file_preset(entry).unwrap();
        assert_eq!(preset.id, "default");

        assert!(find_preset_by_name(&resolved, "missing").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_missing_index_errors() {
        let root = std::env::temp_dir().join(format!("wf-preset-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(resolve_preset_index(&root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_literal_paths_without_glob() {
        let root = std::env::temp_dir().join(format!("wf-preset-literal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("infra");
        std::fs::create_dir_all(&base).unwrap();
        write_json(
            &base.join("index.json"),
            r#"{"version": "1.0", "type": "infrastructure_presets", "paths": ["./dev.json"]}"#,
        );
        write_json(&base.join("dev.json"), r#"{"id": "dev"}"#);

        let resolved = resolve_preset_index(&base).unwrap();
        assert_eq!(resolved.presets.len(), 1);
        assert!(resolved.presets.contains_key("dev"));
        let _ = std::fs::remove_dir_all(&root);
    }
}

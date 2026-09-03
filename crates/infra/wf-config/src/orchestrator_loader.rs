//! Infrastructure preset resolution and per-domain config loading.
//!
//! Turns a preset name into a concrete `files` mapping and reads each
//! config domain file leniently (missing/unparseable falls back to
//! defaults). Isolated from the assembly orchestrator to keep the preset
//! layer independently maintainable.

use std::path::Path;

use crate::error::{ConfigError, ConfigResult};
use crate::layered;
use crate::orchestrator::InfrastructurePresetFiles;
use crate::preset::{
    find_preset_by_name, load_single_file_preset, resolve_preset_index, INDEX_FILE_NAME,
};
use tracing::warn;

pub fn load_infrastructure_preset(
    infra_dir: &Path,
    preset_name: &str,
) -> ConfigResult<InfrastructurePresetFiles> {
    let resolved = resolve_preset_index(infra_dir)?;
    let entry = find_preset_by_name(&resolved, preset_name).ok_or_else(|| {
        ConfigError::NotFound(format!(
            "Infrastructure preset '{preset_name}' not found in {}",
            infra_dir.join(INDEX_FILE_NAME).display()
        ))
    })?;
    let value = load_single_file_preset::<serde_json::Value>(entry)?;
    let files = value.get("files").ok_or_else(|| {
        ConfigError::Validation(format!(
            "Infrastructure preset '{preset_name}' has no 'files' mapping"
        ))
    })?;
    let files = files.as_object().ok_or_else(|| {
        ConfigError::Validation(format!(
            "Infrastructure preset '{preset_name}' 'files' must be an object"
        ))
    })?;

    let base_dir = entry.file_path.parent().unwrap_or(infra_dir);
    let mut mapping = InfrastructurePresetFiles::default();
    for (key, target) in files {
        let target = target.as_str().ok_or_else(|| {
            ConfigError::Validation(format!(
                "Infrastructure preset '{preset_name}' file path for '{key}' must be a string"
            ))
        })?;
        let path = base_dir.join(target);
        match key.as_str() {
            "storage" => mapping.storage = path.to_string_lossy().to_string(),
            "timeout" => mapping.timeout = path.to_string_lossy().to_string(),
            "metrics" => mapping.metrics = path.to_string_lossy().to_string(),
            "output" => mapping.output = path.to_string_lossy().to_string(),
            "sandbox" => mapping.sandbox = path.to_string_lossy().to_string(),
            "file_checkpoint" => mapping.file_checkpoint = path.to_string_lossy().to_string(),
            "tool_approval" => mapping.tool_approval = path.to_string_lossy().to_string(),
            "presets" => mapping.presets = path.to_string_lossy().to_string(),
            "tools" => mapping.tools = path.to_string_lossy().to_string(),
            "limits" => mapping.limits = path.to_string_lossy().to_string(),
            _ => {}
        }
    }
    Ok(mapping)
}

/// Resolve the file mapping for each domain:
/// 1. no `index.json` -> default paths (or fixed filenames when absent);
/// 2. preset name given and found -> the preset's `files` mapping;
/// 3. otherwise -> default paths (or fixed filenames when absent).
pub(crate) fn resolve_file_mapping(
    infra_dir: &Path,
    preset_name: Option<&str>,
    default_paths: Option<InfrastructurePresetFiles>,
) -> InfrastructurePresetFiles {
    // Legacy fallback (no explicit default paths): fixed default filenames.
    let fallback = || {
        default_paths
            .clone()
            .unwrap_or_else(InfrastructurePresetFiles::default_filenames)
    };
    let index_path = infra_dir.join(INDEX_FILE_NAME);
    if !index_path.exists() {
        return fallback();
    }
    match preset_name {
        Some(name) => match load_infrastructure_preset(infra_dir, name) {
            Ok(files) => files,
            Err(e) => {
                warn!(error = %e, "failed to resolve infrastructure preset '{name}'; falling back to default file mapping");
                fallback()
            }
        },
        None => fallback(),
    }
}

/// Load a single config domain file leniently: a missing or unparseable
/// file falls back to the domain defaults (with a warning).
pub(crate) fn load_domain_config<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    if !path.exists() {
        return T::default();
    }
    match layered::load_layered_config_sync::<T>(&[path]) {
        Ok(config) => config,
        Err(e) => {
            warn!(
                error = %e,
                "failed to parse config file {}; falling back to defaults",
                path.display()
            );
            T::default()
        }
    }
}

/// Convert camelCase keys to snake_case recursively (tools.toml may use
/// either casing, e.g. `maxResults` or `max_results`).
pub(crate) fn normalize_camel_case(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                let mut snake = String::new();
                for (i, c) in key.chars().enumerate() {
                    if c.is_uppercase() {
                        if i > 0 {
                            snake.push('_');
                        }
                        snake.push(c.to_ascii_lowercase());
                    } else {
                        snake.push(c);
                    }
                }
                out.insert(snake, normalize_camel_case(val));
            }
            serde_json::Value::Object(out)
        }
        other => other,
    }
}

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
/// without an index file or preset name the default paths apply; an
/// explicitly requested preset that cannot be resolved fails instead of
/// silently falling back to unrelated files.
pub(crate) fn resolve_file_mapping(
    infra_dir: &Path,
    preset_name: Option<&str>,
    default_paths: Option<InfrastructurePresetFiles>,
) -> ConfigResult<InfrastructurePresetFiles> {
    let fallback = || {
        default_paths
            .clone()
            .unwrap_or_else(InfrastructurePresetFiles::default_filenames)
    };
    let index_path = infra_dir.join(INDEX_FILE_NAME);
    if !index_path.exists() {
        return Ok(fallback());
    }
    match preset_name {
        Some(name) => load_infrastructure_preset(infra_dir, name).map_err(|e| {
            ConfigError::NotFound(format!(
                "infrastructure preset '{name}' cannot be resolved: {e}"
            ))
        }),
        None => Ok(fallback()),
    }
}

/// Load a single config domain file. A missing file yields the
/// caller-provided default; a present but unparseable file fails so typos
/// cannot silently run with defaults.
pub(crate) fn load_domain_config_with_metrics<T>(
    path: &Path,
    env_default: T,
    metrics: Option<&wf_metrics::ConfigMetricsCollector>,
) -> ConfigResult<T>
where
    T: serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(env_default);
    }
    match layered::load_layered_config_sync_with_metrics::<T>(&[path], metrics) {
        Ok(config) => Ok(config),
        Err(ConfigError::NotFound(_)) => Ok(env_default),
        Err(e) => Err(ConfigError::Parse(format!(
            "invalid config file {}: {e}",
            path.display()
        ))),
    }
}

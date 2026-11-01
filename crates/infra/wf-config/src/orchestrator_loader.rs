//! Per-domain infrastructure config loading.
//!
//! Each domain file resolves from the explicit override, user settings and
//! project layers (low to high). Missing files yield the caller-provided
//! default; a present but unparseable file fails so typos cannot silently
//! run with defaults.

use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult};
use crate::layered;

/// Load one config domain from candidate files in precedence order (low to
/// high). Files that do not exist are skipped; when none exists the
/// caller-provided default wins. A present but unparseable file fails.
pub(crate) fn load_merged_domain_config_with_metrics<T>(
    paths: &[PathBuf],
    env_default: T,
    metrics: Option<&wf_metrics::ConfigMetricsCollector>,
) -> ConfigResult<T>
where
    T: serde::de::DeserializeOwned,
{
    let existing: Vec<&Path> = paths
        .iter()
        .map(PathBuf::as_path)
        .filter(|path| path.exists())
        .collect();
    if existing.is_empty() {
        return Ok(env_default);
    }
    match layered::load_layered_config_sync_with_metrics::<T>(&existing, metrics) {
        Ok(config) => Ok(config),
        Err(ConfigError::NotFound(_)) => Ok(env_default),
        Err(e) => Err(ConfigError::Parse(format!(
            "invalid infrastructure config files {}: {e}",
            display_paths(&existing)
        ))),
    }
}

/// Comma-joined display of candidate paths for error messages.
pub(crate) fn display_paths(paths: &[&Path]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

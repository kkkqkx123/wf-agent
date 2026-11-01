//! Per-domain infrastructure config loading.
//!
//! Reads each config domain file leniently from the fixed filenames under
//! `baselines/infrastructure/`. A missing file yields the caller-provided
//! code default; a present but unparseable file fails so typos cannot
//! silently run with defaults.

use std::path::Path;

use crate::error::{ConfigError, ConfigResult};
use crate::layered;

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

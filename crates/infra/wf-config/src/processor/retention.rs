use crate::error::{ConfigError, ConfigResult};
use wf_types::config::RetentionConfig;

/// Materialize a retention config with built-in defaults.
///
/// Serde defaults already fill missing fields when a TOML section is absent;
/// this entry keeps validation and future non-serde defaults in one place.
pub fn merge_retention_with_defaults(user: &RetentionConfig) -> RetentionConfig {
    user.clone()
}

pub fn validate_retention_config(config: &RetentionConfig) -> ConfigResult<()> {
    fn positive(value: u64, name: &str) -> ConfigResult<()> {
        if value == 0 {
            return Err(ConfigError::Validation(format!(
                "{name} must be greater than zero"
            )));
        }
        Ok(())
    }

    positive(config.scan_interval_secs, "retention.scan_interval_secs")?;
    positive(config.batch_size as u64, "retention.batch_size")?;
    positive(
        config.checkpoints.max_age_days,
        "retention.checkpoints.max_age_days",
    )?;
    positive(
        config.checkpoints.max_checkpoints as u64,
        "retention.checkpoints.max_checkpoints",
    )?;
    positive(
        config.file_checkpoints.history_max_age_days,
        "retention.file_checkpoints.history_max_age_days",
    )?;
    positive(
        config.executions.history_retention_days,
        "retention.executions.history_retention_days",
    )?;
    positive(
        config.tasks.retention_days,
        "retention.tasks.retention_days",
    )?;
    positive(
        config.tasks.trigger_retention_days,
        "retention.tasks.trigger_retention_days",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_conservative() {
        let config = RetentionConfig::default();
        assert!(!config.enabled);
        assert!(config.dry_run);
        assert!(validate_retention_config(&config).is_ok());
    }

    #[test]
    fn zero_interval_is_rejected() {
        let config = RetentionConfig {
            scan_interval_secs: 0,
            ..Default::default()
        };
        assert!(validate_retention_config(&config).is_err());
    }
}

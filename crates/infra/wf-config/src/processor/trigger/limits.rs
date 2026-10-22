use super::*;

/// Validate shared runtime limits (concurrency gate + circuit breaker).
pub fn validate_trigger_runtime_limits(limits: &TriggerRuntimeLimits) -> ConfigResult<()> {
    limits.validate().map_err(ConfigError::Validation)
}

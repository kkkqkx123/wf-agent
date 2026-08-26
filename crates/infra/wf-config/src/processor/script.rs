use crate::error::ConfigResult;
use crate::validator::validate_required;

use wf_types::script::executor::ScriptExecutorConfig;

pub fn validate_script_executor(config: &ScriptExecutorConfig) -> ConfigResult<()> {
    validate_required(&config.executor_type, "executor_type")?;
    Ok(())
}

pub fn transform_script_executor(config: &ScriptExecutorConfig) -> ScriptExecutorConfig {
    config.clone()
}

pub fn export_script_executor(config: ScriptExecutorConfig) -> ScriptExecutorConfig {
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> ScriptExecutorConfig {
        ScriptExecutorConfig {
            executor_type: "docker".to_string(),
            timeout_seconds: Some(30),
            max_memory_mb: Some(512),
            allowed_paths: None,
        }
    }

    #[test]
    fn test_valid_config() {
        let config = make_config();
        assert!(validate_script_executor(&config).is_ok());
    }

    #[test]
    fn test_empty_executor_type() {
        let mut config = make_config();
        config.executor_type = String::new();
        assert!(validate_script_executor(&config).is_err());
    }

    #[test]
    fn test_transform_script_executor() {
        let config = make_config();
        let transformed = transform_script_executor(&config);
        assert_eq!(transformed.executor_type, "docker");
        assert_eq!(transformed.timeout_seconds, Some(30));
    }

    #[test]
    fn test_export_script_executor() {
        let config = make_config();
        let exported = export_script_executor(config.clone());
        assert_eq!(exported.executor_type, config.executor_type);
    }
}

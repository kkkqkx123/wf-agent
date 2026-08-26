use std::collections::HashMap;

use crate::error::ConfigResult;
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_min;

use wf_types::script::interactive::InteractiveScriptConfig;

pub fn validate_interactive_script(config: &InteractiveScriptConfig) -> ConfigResult<()> {
    if let Some(timeout) = config.timeout_seconds {
        validate_min(timeout, 1, "timeout_seconds")?;
    }
    Ok(())
}

pub fn transform_interactive_script(
    config: &InteractiveScriptConfig,
    parameters: &HashMap<String, String>,
) -> ConfigResult<InteractiveScriptConfig> {
    let mut cloned = config.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_interactive_script(config: InteractiveScriptConfig) -> InteractiveScriptConfig {
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> InteractiveScriptConfig {
        InteractiveScriptConfig {
            enabled: true,
            timeout_seconds: Some(60),
            allow_user_input: Some(true),
        }
    }

    #[test]
    fn test_valid_config() {
        let config = make_config();
        assert!(validate_interactive_script(&config).is_ok());
    }

    #[test]
    fn test_zero_timeout() {
        let mut config = make_config();
        config.timeout_seconds = Some(0);
        assert!(validate_interactive_script(&config).is_err());
    }

    #[test]
    fn test_transform_interactive_script() {
        let config = make_config();
        let mut params = HashMap::new();
        params.insert("prompt".to_string(), "Enter value".to_string());

        let result = transform_interactive_script(&config, &params).unwrap();
        assert!(result.enabled);
    }

    #[test]
    fn test_export_interactive_script() {
        let config = make_config();
        let exported = export_interactive_script(config.clone());
        assert_eq!(exported.enabled, config.enabled);
    }
}

use std::collections::HashMap;

use regex::Regex;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_min;

use wf_script::InteractiveScriptConfig;

pub fn validate_interactive_script(config: &InteractiveScriptConfig) -> ConfigResult<()> {
    if let Some(max_rounds) = config.max_rounds {
        validate_min(max_rounds, 1, "max_rounds")?;
    }
    if let Some(timeout) = config.round_timeout {
        validate_min(timeout, 1, "round_timeout")?;
    }
    if let Some(patterns) = &config.prompt_patterns {
        for pattern in patterns {
            Regex::new(pattern).map_err(|e| {
                ConfigError::Validation(format!("invalid prompt pattern '{pattern}': {e}"))
            })?;
        }
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
    use wf_script::InteractionMode;

    fn make_config() -> InteractiveScriptConfig {
        InteractiveScriptConfig {
            mode: InteractionMode::Blocking,
            max_rounds: Some(10),
            interaction_points: None,
            prompt_patterns: None,
            round_timeout: Some(30_000),
        }
    }

    #[test]
    fn test_valid_config() {
        let config = make_config();
        assert!(validate_interactive_script(&config).is_ok());
    }

    #[test]
    fn test_zero_rounds() {
        let mut config = make_config();
        config.max_rounds = Some(0);
        assert!(validate_interactive_script(&config).is_err());
    }

    #[test]
    fn test_invalid_prompt_pattern() {
        let mut config = make_config();
        config.prompt_patterns = Some(vec!["(unclosed".to_string()]);
        assert!(validate_interactive_script(&config).is_err());
    }

    #[test]
    fn test_transform_interactive_script() {
        let config = make_config();
        let mut params = HashMap::new();
        params.insert("prompt".to_string(), "Enter value".to_string());

        let result = transform_interactive_script(&config, &params).unwrap();
        assert_eq!(result.max_rounds, Some(10));
    }

    #[test]
    fn test_export_interactive_script() {
        let config = make_config();
        let exported = export_interactive_script(config.clone());
        assert_eq!(exported.max_rounds, config.max_rounds);
    }
}

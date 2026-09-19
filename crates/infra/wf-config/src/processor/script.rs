use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_required;

use wf_script::ScriptDefinition;

pub fn validate_script_definition(definition: &ScriptDefinition) -> ConfigResult<()> {
    validate_required(&definition.name, "name")?;
    if definition.enabled != Some(false) {
        wf_script::ScriptEngine::validate_definition_shape(definition)
            .map_err(|e| ConfigError::Validation(e.to_string()))?;
    }
    if let Some(args) = &definition.arguments {
        for arg in args {
            validate_required(&arg.key, "arguments.key")?;
        }
    }
    Ok(())
}

pub fn transform_script_definition(
    definition: &ScriptDefinition,
    parameters: &HashMap<String, String>,
) -> ConfigResult<ScriptDefinition> {
    let mut cloned = definition.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_script_definition(definition: ScriptDefinition) -> ScriptDefinition {
    definition
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_definition() -> ScriptDefinition {
        ScriptDefinition {
            name: "deploy".to_string(),
            content: Some("echo hi".to_string()),
            template: None,
            arguments: None,
            language: Some("shell".to_string()),
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        }
    }

    #[test]
    fn test_valid_definition() {
        let definition = make_definition();
        assert!(validate_script_definition(&definition).is_ok());
    }

    #[test]
    fn test_empty_name() {
        let mut definition = make_definition();
        definition.name = String::new();
        assert!(validate_script_definition(&definition).is_err());
    }

    #[test]
    fn test_missing_content_and_template() {
        let mut definition = make_definition();
        definition.content = None;
        assert!(validate_script_definition(&definition).is_err());
    }

    #[test]
    fn test_disabled_skips_content_check() {
        let mut definition = make_definition();
        definition.content = None;
        definition.enabled = Some(false);
        assert!(validate_script_definition(&definition).is_ok());
    }

    #[test]
    fn test_transform_script_definition() {
        let definition = make_definition();
        let mut params = HashMap::new();
        params.insert("env".to_string(), "prod".to_string());

        let result = transform_script_definition(&definition, &params).unwrap();
        assert_eq!(result.name, "deploy");
    }

    #[test]
    fn test_export_script_definition() {
        let definition = make_definition();
        let exported = export_script_definition(definition.clone());
        assert_eq!(exported.name, definition.name);
    }
}

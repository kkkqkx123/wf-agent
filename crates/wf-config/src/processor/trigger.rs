use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_required;

use wf_types::trigger::template::TriggerTemplate;

pub fn validate_trigger_template(template: &TriggerTemplate) -> ConfigResult<()> {
    validate_required(&template.name, "name")?;
    if let Some(condition) = &template.condition {
        // A NODE_CUSTOM_EVENT condition is matched by `event_name`: it is
        // required, otherwise the template can never match.
        if condition.event_type == "NODE_CUSTOM_EVENT" && condition.event_name.is_none() {
            return Err(ConfigError::Validation(
                "event_name is required when event_type is NODE_CUSTOM_EVENT".to_string(),
            ));
        }
        // A condition expression is only meaningful together with a concrete
        // event type; standalone expressions against every event are
        // rejected as a likely configuration mistake.
        if condition.condition.is_some() && condition.event_type.is_empty() {
            return Err(ConfigError::Validation(
                "condition expression requires a concrete event_type".to_string(),
            ));
        }
    }
    Ok(())
}

pub fn transform_trigger_template(
    template: &TriggerTemplate,
    parameters: &HashMap<String, String>,
) -> ConfigResult<TriggerTemplate> {
    let mut cloned = template.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_trigger_template(template: TriggerTemplate) -> TriggerTemplate {
    template
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_template() -> TriggerTemplate {
        TriggerTemplate {
            name: "on-file-change".to_string(),
            description: None,
            condition: None,
            action: None,
            enabled: None,
            max_triggers: None,
            priority: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    #[test]
    fn test_valid_template() {
        let template = make_template();
        assert!(validate_trigger_template(&template).is_ok());
    }

    #[test]
    fn test_empty_name() {
        let mut template = make_template();
        template.name = String::new();
        assert!(validate_trigger_template(&template).is_err());
    }

    #[test]
    fn test_transform_trigger_template() {
        let template = make_template();
        let mut params = HashMap::new();
        params.insert("target".to_string(), "main".to_string());

        let result = transform_trigger_template(&template, &params).unwrap();
        assert_eq!(result.name, "on-file-change");
    }

    #[test]
    fn test_export_trigger_template() {
        let template = make_template();
        let exported = export_trigger_template(template.clone());
        assert_eq!(exported.name, template.name);
    }
}

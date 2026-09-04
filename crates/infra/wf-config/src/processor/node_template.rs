use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_required;

use wf_types::node::StaticNodeType;
use wf_types::workflow::node_template::NodeTemplate;

pub fn validate_node_template(template: &NodeTemplate) -> ConfigResult<()> {
    validate_required(&template.id, "id")?;
    validate_required(&template.name, "name")?;
    validate_required(&template.node_type, "node_type")?;
    if StaticNodeType::from_str_ci(&template.node_type).is_none() {
        return Err(ConfigError::Validation(format!(
            "unknown node_type '{}'; expected one of: {}",
            template.node_type,
            StaticNodeType::ALL.join(", ")
        )));
    }
    if let Some(ref config) = template.default_config {
        validate_node_default_config(&template.node_type, config)?;
    }
    Ok(())
}

fn validate_node_default_config(node_type: &str, config: &serde_json::Value) -> ConfigResult<()> {
    let issues = crate::processor::node_config::validate_node_config(
        node_type,
        "default_config",
        Some(config),
    );
    if issues.is_empty() {
        Ok(())
    } else {
        let detail = issues
            .iter()
            .map(|i| format!("{}: {}", i.field, i.message))
            .collect::<Vec<_>>()
            .join("; ");
        Err(ConfigError::Validation(detail))
    }
}

pub fn transform_node_template(
    template: &NodeTemplate,
    parameters: &HashMap<String, String>,
) -> ConfigResult<NodeTemplate> {
    let mut cloned = template.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_node_template(template: NodeTemplate) -> NodeTemplate {
    template
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_template() -> NodeTemplate {
        NodeTemplate {
            id: "node-1".to_string(),
            name: "Code Review".to_string(),
            description: "Reviews code".to_string(),
            node_type: "LLM".to_string(),
            default_config: None,
        }
    }

    #[test]
    fn test_valid_template() {
        let template = make_template();
        assert!(validate_node_template(&template).is_ok());
    }

    #[test]
    fn test_empty_id() {
        let mut template = make_template();
        template.id = String::new();
        assert!(validate_node_template(&template).is_err());
    }

    #[test]
    fn test_empty_node_type() {
        let mut template = make_template();
        template.node_type = String::new();
        assert!(validate_node_template(&template).is_err());
    }

    #[test]
    fn test_invalid_node_type_rejected() {
        let mut template = make_template();
        template.node_type = "NOT_A_TYPE".to_string();
        assert!(validate_node_template(&template).is_err());
    }

    #[test]
    fn test_case_insensitive_node_type() {
        let mut template = make_template();
        template.node_type = "llm".to_string();
        assert!(validate_node_template(&template).is_ok());
    }

    #[test]
    fn test_default_config_validates_against_node_type() {
        let template = NodeTemplate {
            id: "nt-1".to_string(),
            name: "LLM Template".to_string(),
            description: String::new(),
            node_type: "LLM".to_string(),
            default_config: Some(serde_json::json!({"profile_id": "valid-profile"})),
        };
        assert!(validate_node_template(&template).is_ok());
    }

    #[test]
    fn test_default_config_invalid_for_node_type() {
        let template = NodeTemplate {
            id: "nt-2".to_string(),
            name: "LLM Bad".to_string(),
            description: String::new(),
            node_type: "LLM".to_string(),
            default_config: Some(serde_json::json!({})),
        };
        assert!(validate_node_template(&template).is_err());
    }

    #[test]
    fn test_transform_node_template() {
        let template = make_template();
        let mut params = HashMap::new();
        params.insert("env".to_string(), "prod".to_string());

        let result = transform_node_template(&template, &params).unwrap();
        assert_eq!(result.id, "node-1");
        assert_eq!(result.name, "Code Review");
    }

    #[test]
    fn test_export_node_template() {
        let template = make_template();
        let exported = export_node_template(template.clone());
        assert_eq!(exported.id, template.id);
        assert_eq!(exported.name, template.name);
    }
}

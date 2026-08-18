use std::collections::HashMap;

use crate::error::ConfigResult;
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_required;

use wf_types::workflow::node_template::NodeTemplate;

pub fn validate_node_template(template: &NodeTemplate) -> ConfigResult<()> {
    validate_required(&template.id, "id")?;
    validate_required(&template.name, "name")?;
    validate_required(&template.node_type, "node_type")?;
    Ok(())
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
            node_type: "llm".to_string(),
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

use serde_json::Value;

use super::common::{
    field_not_in, field_path, is_valid_identifier, require_string,
    validate_embedded_variable_paths, NodeConfigIssue,
};

pub(crate) fn validate_variable_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    if let Some(config) = config.and_then(|c| c.as_object()) {
        let config = Value::Object(config.clone());
        if let Some(err) = require_string(node_id, node_type, &config, "variable_name", true) {
            errors.push(err);
        } else if let Some(name) = config.get("variable_name").and_then(|v| v.as_str()) {
            if !is_valid_identifier(name) {
                errors.push(NodeConfigIssue::new(
                    field_path(node_id, "variable_name"),
                    format!(
                        "Node '{}' ({}) field 'variable_name' has invalid value '{}'; must start with a letter or '_' and contain only letters, digits or '_'",
                        node_id, node_type, name
                    ),
                ));
            }
        }
        if let Some(err) = require_string(node_id, node_type, &config, "expression", true) {
            errors.push(err);
        } else if let Some(expression) = config.get("expression").and_then(|v| v.as_str()) {
            for problem in validate_embedded_variable_paths(expression) {
                errors.push(NodeConfigIssue::new(
                    field_path(node_id, "expression"),
                    format!("Node '{}' ({}) has {}", node_id, node_type, problem),
                ));
            }
        }
        if let Some(err) = field_not_in(
            node_id,
            node_type,
            &config,
            "variable_type",
            &["number", "string", "boolean", "array", "object"],
        ) {
            errors.push(err);
        }
        if let Some(err) = field_not_in(
            node_id,
            node_type,
            &config,
            "scope",
            &["global", "workflow_execution", "subgraph", "loop"],
        ) {
            errors.push(err);
        }
    }
    errors
}

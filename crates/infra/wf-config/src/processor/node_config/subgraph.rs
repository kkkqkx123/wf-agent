use serde_json::Value;

use super::common::{
    field_path, node_path, require_string, validate_internal_name, validate_variable_path,
    NodeConfigIssue,
};

pub(crate) fn validate_subgraph_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("SUBGRAPH node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    let has_subgraph_id = config
        .get("subgraph_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    let has_embed_id = config
        .get("embed_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    if !has_subgraph_id && !has_embed_id {
        errors.push(NodeConfigIssue::new(
            field_path(node_id, "subgraph_id"),
            format!(
                "SUBGRAPH node '{}' must define a non-empty subgraph_id (or embed_id)",
                node_id
            ),
        ));
    }

    if let Some(inputs) = config.get("variable_inputs").and_then(|v| v.as_array()) {
        let mut seen_internal = std::collections::HashSet::new();
        for (idx, input) in inputs.iter().enumerate() {
            let prefix = format!("nodes.{}.config.variable_inputs[{}]", node_id, idx);
            let input = Value::Object(input.as_object().cloned().unwrap_or_default());
            if let Some(err) = require_string(node_id, node_type, &input, "source_path", true) {
                errors.push(NodeConfigIssue::new(prefix.clone(), err.message));
            } else if let Some(source) = input.get("source_path").and_then(|v| v.as_str()) {
                if let Some(reason) = validate_variable_path(source) {
                    errors.push(NodeConfigIssue::new(
                        format!("{}.source_path", prefix),
                        format!(
                            "SUBGRAPH node '{}' has invalid source_path '{}': {}",
                            node_id, source, reason
                        ),
                    ));
                }
            }
            if let Some(err) = require_string(node_id, node_type, &input, "internal_name", true) {
                errors.push(NodeConfigIssue::new(prefix.clone(), err.message));
            } else if let Some(name) = input.get("internal_name").and_then(|v| v.as_str()) {
                if let Some(reason) = validate_internal_name(name) {
                    errors.push(NodeConfigIssue::new(
                        format!("{}.internal_name", prefix),
                        format!(
                            "SUBGRAPH node '{}' has invalid internal_name: {}",
                            node_id, reason
                        ),
                    ));
                } else if !seen_internal.insert(name.to_string()) {
                    errors.push(NodeConfigIssue::new(
                        format!("{}.internal_name", prefix),
                        format!(
                            "SUBGRAPH node '{}' maps internal_name '{}' more than once",
                            node_id, name
                        ),
                    ));
                }
            }
        }
    }

    if let Some(outputs) = config.get("variable_outputs").and_then(|v| v.as_array()) {
        for (idx, output) in outputs.iter().enumerate() {
            let prefix = format!("nodes.{}.config.variable_outputs[{}]", node_id, idx);
            let output = Value::Object(output.as_object().cloned().unwrap_or_default());
            if let Some(err) = require_string(node_id, node_type, &output, "internal_name", true) {
                errors.push(NodeConfigIssue::new(prefix.clone(), err.message));
            } else if let Some(name) = output.get("internal_name").and_then(|v| v.as_str()) {
                if let Some(reason) = validate_internal_name(name) {
                    errors.push(NodeConfigIssue::new(
                        format!("{}.internal_name", prefix),
                        format!(
                            "SUBGRAPH node '{}' has invalid internal_name: {}",
                            node_id, reason
                        ),
                    ));
                }
            }
            if let Some(err) = require_string(node_id, node_type, &output, "target_path", true) {
                errors.push(NodeConfigIssue::new(prefix.clone(), err.message));
            } else if let Some(target) = output.get("target_path").and_then(|v| v.as_str()) {
                if let Some(reason) = validate_variable_path(target) {
                    errors.push(NodeConfigIssue::new(
                        format!("{}.target_path", prefix),
                        format!(
                            "SUBGRAPH node '{}' has invalid target_path '{}': {}",
                            node_id, target, reason
                        ),
                    ));
                }
            }
        }
    }
    errors
}

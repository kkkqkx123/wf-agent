use serde_json::Value;

use super::common::{field_not_in, field_path, node_path, require_string, NodeConfigIssue};

pub(crate) fn validate_user_interaction_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("USER_INTERACTION node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    if let Some(err) = require_string(node_id, node_type, &config, "prompt", true) {
        errors.push(err);
    }
    if let Some(err) = require_string(node_id, node_type, &config, "operation_type", true) {
        errors.push(err);
    }
    if let Some(err) = field_not_in(
        node_id,
        node_type,
        &config,
        "operation_type",
        &["update_variables", "add_message"],
    ) {
        errors.push(err);
    }
    errors
}

pub(crate) fn validate_tool_visibility_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("TOOL_VISIBILITY node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    if let Some(action) = config.get("action").filter(|v| !v.is_null()) {
        match action.as_str() {
            Some("block") | Some("unblock") => {}
            Some(other) => errors.push(NodeConfigIssue::new(
                field_path(node_id, "action"),
                format!(
                    "Node '{}' ({}) field 'action' has invalid value '{}', expected one of block, unblock",
                    node_id, node_type, other
                ),
            )),
            None => errors.push(NodeConfigIssue::new(
                field_path(node_id, "action"),
                format!(
                    "Node '{}' ({}) field 'action' must be a non-empty string",
                    node_id, node_type
                ),
            )),
        }
    }
    match config.get("tool_ids").and_then(|v| v.as_array()) {
        None => errors.push(NodeConfigIssue::new(
            field_path(node_id, "tool_ids"),
            format!(
                "TOOL_VISIBILITY node '{}' must define a non-empty tool_ids array",
                node_id
            ),
        )),
        Some(arr) if arr.is_empty() => errors.push(NodeConfigIssue::new(
            field_path(node_id, "tool_ids"),
            format!(
                "TOOL_VISIBILITY node '{}' must define a non-empty tool_ids array",
                node_id
            ),
        )),
        Some(arr) => {
            let mut seen = std::collections::HashSet::new();
            for (idx, entry) in arr.iter().enumerate() {
                match entry.as_str() {
                    Some(name) if !name.trim().is_empty() => {
                        if !seen.insert(name.to_string()) {
                            errors.push(NodeConfigIssue::new(
                                format!("nodes.{}.config.tool_ids[{}]", node_id, idx),
                                format!(
                                    "TOOL_VISIBILITY node '{}' lists tool '{}' more than once",
                                    node_id, name
                                ),
                            ));
                        }
                    }
                    _ => errors.push(NodeConfigIssue::new(
                        format!("nodes.{}.config.tool_ids[{}]", node_id, idx),
                        format!(
                            "TOOL_VISIBILITY node '{}' has an invalid tool_ids entry; each entry must be a non-empty string",
                            node_id
                        ),
                    )),
                }
            }
        }
    }
    errors
}

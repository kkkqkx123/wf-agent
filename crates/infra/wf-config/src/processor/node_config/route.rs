use serde_json::Value;

use super::common::{
    field_path, node_path, require_string, validate_embedded_variable_paths, NodeConfigIssue,
};

pub(crate) fn validate_route_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("ROUTE node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    let conditions = config.get("conditions").and_then(|v| v.as_array());
    let has_conditions = conditions.is_some_and(|arr| !arr.is_empty());
    let has_default = config
        .get("default_target_node_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty());
    if !has_conditions && !has_default {
        errors.push(NodeConfigIssue::new(
            field_path(node_id, "conditions"),
            format!(
                "ROUTE node '{}' must define a non-empty conditions array or a default_target_node_id",
                node_id
            ),
        ));
    }
    if let Some(default) = config
        .get("default_target_node_id")
        .filter(|v| !v.is_null())
    {
        match default.as_str() {
            Some(s) if !s.trim().is_empty() => {}
            _ => errors.push(NodeConfigIssue::new(
                field_path(node_id, "default_target_node_id"),
                format!(
                    "Node '{}' ({}) field 'default_target_node_id' must be a non-empty string",
                    node_id, node_type
                ),
            )),
        }
    }

    if let Some(conditions) = conditions {
        for (idx, condition) in conditions.iter().enumerate() {
            let prefix = format!("nodes.{}.config.conditions[{}]", node_id, idx);
            if let Some(err) = require_string(
                node_id,
                node_type,
                &Value::Object(condition.as_object().cloned().unwrap_or_default()),
                "expression",
                true,
            ) {
                errors.push(NodeConfigIssue::new(prefix.clone(), err.message));
            } else if let Some(expression) = condition.get("expression").and_then(|v| v.as_str()) {
                if let Err(reason) =
                    wf_core::condition::ConditionEvaluator::validate_syntax(expression)
                {
                    errors.push(NodeConfigIssue::new(
                        format!("{}.expression", prefix),
                        format!(
                            "ROUTE node '{}' has an invalid condition expression: {}",
                            node_id, reason
                        ),
                    ));
                }
                for problem in validate_embedded_variable_paths(expression) {
                    errors.push(NodeConfigIssue::new(
                        format!("{}.expression", prefix),
                        format!("ROUTE node '{}' has {}", node_id, problem),
                    ));
                }
            }
            if let Some(err) = require_string(
                node_id,
                node_type,
                &Value::Object(condition.as_object().cloned().unwrap_or_default()),
                "target_node_id",
                true,
            ) {
                errors.push(NodeConfigIssue::new(prefix.clone(), err.message));
            }
            if let (Some(target), Some(default)) = (
                condition.get("target_node_id").and_then(|v| v.as_str()),
                config
                    .get("default_target_node_id")
                    .and_then(|v| v.as_str()),
            ) {
                if !target.is_empty() && target == default {
                    errors.push(NodeConfigIssue::new(
                        prefix.clone(),
                        format!(
                            "ROUTE node '{}' condition target '{}' duplicates the default target; use distinct targets",
                            node_id, target
                        ),
                    ));
                }
            }
        }
    }
    errors
}

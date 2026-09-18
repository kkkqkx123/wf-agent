use serde_json::Value;

use wf_types::llm::{
    DeadLoopDetectionConfig, LlmGenerationParams, ToolCallProtocolViolationPolicy,
};

use super::common::{
    field_path, require_string, validate_execution_settings, validate_typed_field, NodeConfigIssue,
};

/// Reject an explicit zero interaction budget: zero rounds would run no
/// model call and return an empty result, which masks a misconfiguration.
/// Non-integer values are owned by the execution-settings type check, so
/// only the parsed zero is flagged here.
fn reject_zero_budget(
    node_id: &str,
    node_type: &str,
    config: &Value,
    field: &str,
) -> Option<NodeConfigIssue> {
    let zero = config
        .get(field)
        .and_then(|v| v.as_u64())
        .is_some_and(|n| n == 0);
    if !zero {
        return None;
    }
    Some(NodeConfigIssue::new(
        field_path(node_id, field),
        format!("Node '{node_id}' ({node_type}) field '{field}' must be >= 1"),
    ))
}

pub(crate) fn validate_llm_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    if let Some(config) = config.and_then(|c| c.as_object()) {
        let config = Value::Object(config.clone());
        if let Some(err) = require_string(node_id, node_type, &config, "profile_id", true) {
            errors.push(err);
        }
        if let Some(err) = validate_typed_field::<ToolCallProtocolViolationPolicy>(
            node_id,
            node_type,
            &config,
            "violation_policy",
        ) {
            errors.push(err);
        }
        if let Some(err) = validate_typed_field::<DeadLoopDetectionConfig>(
            node_id,
            node_type,
            &config,
            "dead_loop_detection",
        ) {
            errors.push(err);
        }
        if let Some(err) =
            validate_typed_field::<LlmGenerationParams>(node_id, node_type, &config, "generation")
        {
            errors.push(err);
        }
        if let Some(err) = reject_zero_budget(node_id, node_type, &config, "max_interactions") {
            errors.push(err);
        }
        if let Some(err) =
            reject_zero_budget(node_id, node_type, &config, "max_tool_calls_per_request")
        {
            errors.push(err);
        }
        if let Some(err) = validate_execution_settings(node_id, node_type, &config) {
            errors.push(err);
        }
    }
    errors
}

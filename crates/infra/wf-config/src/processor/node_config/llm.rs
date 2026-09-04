use serde_json::Value;

use wf_types::llm::{
    DeadLoopDetectionConfig, LlmGenerationParams, ToolCallProtocolViolationPolicy,
};

use super::common::{
    require_string, validate_execution_settings, validate_typed_field, NodeConfigIssue,
};

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
        if let Some(err) = validate_execution_settings(node_id, node_type, &config) {
            errors.push(err);
        }
    }
    errors
}

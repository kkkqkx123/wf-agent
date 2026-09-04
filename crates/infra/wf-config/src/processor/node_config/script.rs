use serde_json::Value;

use super::common::{field_not_in, require_string, NodeConfigIssue};

pub(crate) fn validate_script_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    if let Some(config) = config.and_then(|c| c.as_object()) {
        let config = Value::Object(config.clone());
        if let Some(err) = require_string(node_id, node_type, &config, "script_name", true) {
            errors.push(err);
        }
        if let Some(err) = require_string(node_id, node_type, &config, "risk", true) {
            errors.push(err);
        }
        if let Some(err) = field_not_in(
            node_id,
            node_type,
            &config,
            "risk",
            &["none", "low", "medium", "high"],
        ) {
            errors.push(err);
        }
    }
    errors
}

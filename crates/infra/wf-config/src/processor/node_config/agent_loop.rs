use serde_json::Value;

use wf_types::agent::AgentConfig;

use super::common::{node_path, validate_execution_settings, NodeConfigIssue};

pub(crate) fn validate_agent_loop_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("AGENT_LOOP node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    let has_loop_id = config
        .get("agent_loop_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    let inline = config.get("inline_definition").filter(|v| !v.is_null());
    if !has_loop_id && inline.is_none() {
        errors.push(NodeConfigIssue::new(
            format!("nodes.{}.config", node_id),
            format!(
                "AGENT_LOOP node '{}' must define agent_loop_id or inline_definition",
                node_id
            ),
        ));
    }
    if let Some(inline) = inline {
        match inline.as_object() {
            None => errors.push(NodeConfigIssue::new(
                format!("nodes.{}.config.inline_definition", node_id),
                format!(
                    "AGENT_LOOP node '{}' has invalid inline_definition: expected an object",
                    node_id
                ),
            )),
            Some(_) => {
                if let Some(agent_config) = inline.get("config").filter(|v| !v.is_null()) {
                    match serde_json::from_value::<AgentConfig>(agent_config.clone()) {
                        Err(e) => errors.push(NodeConfigIssue::new(
                            format!("nodes.{}.config.inline_definition.config", node_id),
                            format!(
                                "AGENT_LOOP node '{}' has invalid inline agent config: {}",
                                node_id, e
                            ),
                        )),
                        Ok(parsed) => {
                            if parsed
                                .profile_id
                                .as_deref()
                                .is_none_or(|s| s.trim().is_empty())
                            {
                                errors.push(NodeConfigIssue::new(
                                    format!(
                                        "nodes.{}.config.inline_definition.config.profile_id",
                                        node_id
                                    ),
                                    format!(
                                        "AGENT_LOOP node '{}' inline config requires a non-empty profile_id",
                                        node_id
                                    ),
                                ));
                            }
                            if parsed.max_iterations.is_some_and(|n| n == 0) {
                                errors.push(NodeConfigIssue::new(
                                    format!(
                                        "nodes.{}.config.inline_definition.config.max_iterations",
                                        node_id
                                    ),
                                    format!(
                                        "AGENT_LOOP node '{}' inline max_iterations must be >= 1",
                                        node_id
                                    ),
                                ));
                            }
                            if let Some(format) = parsed.tool_call_format.as_deref() {
                                if !format.trim().is_empty()
                                    && <wf_types::llm::ToolCallFormat as std::str::FromStr>::from_str(
                                        format,
                                    )
                                    .is_err()
                                {
                                    errors.push(NodeConfigIssue::new(
                                        format!(
                                            "nodes.{}.config.inline_definition.config.tool_call_format",
                                            node_id
                                        ),
                                        format!(
                                            "AGENT_LOOP node '{}' has unknown tool_call_format '{}'; expected one of native, xml, json_wrapped, json_raw",
                                            node_id, format
                                        ),
                                    ));
                                }
                            }
                            if let Some(tools) = agent_config
                                .get("available_tools")
                                .and_then(|v| v.as_object())
                            {
                                for key in ["available", "initial", "discoverable", "hidden"] {
                                    if let Some(list) = tools.get(key).and_then(|v| v.as_array()) {
                                        for (idx, entry) in list.iter().enumerate() {
                                            if entry.as_str().is_none_or(|s| s.trim().is_empty()) {
                                                errors.push(NodeConfigIssue::new(
                                                    format!(
                                                        "nodes.{}.config.inline_definition.config.available_tools.{}[{}]",
                                                        node_id, key, idx
                                                    ),
                                                    format!(
                                                        "AGENT_LOOP node '{}' has an empty tool name; each entry must be a non-empty string",
                                                        node_id
                                                    ),
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    errors.push(NodeConfigIssue::new(
                        format!("nodes.{}.config.inline_definition.config", node_id),
                        format!(
                            "AGENT_LOOP node '{}' inline_definition requires a config block with profile_id",
                            node_id
                        ),
                    ));
                }
            }
        }
    }
    if let Some(err) = validate_execution_settings(node_id, node_type, &config) {
        errors.push(err);
    }
    errors
}

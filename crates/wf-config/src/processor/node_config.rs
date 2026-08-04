use serde_json::Value;

/// A single config issue found on a workflow node. Field carries a dotted
/// path pointing at the offending attribute (e.g. `nodes.n1.config.profile_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeConfigIssue {
    pub field: String,
    pub message: String,
}

impl NodeConfigIssue {
    fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

fn field_path(node_id: &str, field: &str) -> String {
    format!("nodes.{}.config.{}", node_id, field)
}

fn node_path(node_id: &str) -> String {
    format!("nodes.{}", node_id)
}

fn require_string(
    node_id: &str,
    node_type: &str,
    config: &Value,
    field: &str,
    required: bool,
) -> Option<NodeConfigIssue> {
    match config.get(field) {
        Some(Value::String(s)) if !s.trim().is_empty() => None,
        Some(_) => Some(NodeConfigIssue::new(
            field_path(node_id, field),
            format!(
                "Node '{}' ({}) field '{}' must be a non-empty string",
                node_id, node_type, field
            ),
        )),
        None if required => Some(NodeConfigIssue::new(
            field_path(node_id, field),
            format!(
                "Node '{}' ({}) is missing required config '{}'",
                node_id, node_type, field
            ),
        )),
        None => None,
    }
}

fn field_not_in(
    node_id: &str,
    node_type: &str,
    config: &Value,
    field: &str,
    allowed: &[&str],
) -> Option<NodeConfigIssue> {
    let value = config.get(field)?;
    let actual = value.as_str().unwrap_or_default();
    if allowed.contains(&actual) {
        return None;
    }
    Some(NodeConfigIssue::new(
        field_path(node_id, field),
        format!(
            "Node '{}' ({}) field '{}' has invalid value '{}', expected one of {}",
            node_id,
            node_type,
            field,
            actual,
            allowed.join(", ")
        ),
    ))
}

fn validate_llm_node(node_id: &str, node_type: &str, config: Option<&Value>) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    if let Some(config) = config.and_then(|c| c.as_object()) {
        let config = Value::Object(config.clone());
        if let Some(err) = require_string(node_id, node_type, &config, "profile_id", true) {
            errors.push(err);
        }
    }
    errors
}

fn validate_script_node(
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

fn validate_variable_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    if let Some(config) = config.and_then(|c| c.as_object()) {
        let config = Value::Object(config.clone());
        if let Some(err) = require_string(node_id, node_type, &config, "variable_name", true) {
            errors.push(err);
        }
        if let Some(err) = require_string(node_id, node_type, &config, "expression", true) {
            errors.push(err);
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

fn validate_route_node(
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

    match config.get("conditions").and_then(|v| v.as_array()) {
        Some(arr) if arr.is_empty() => errors.push(NodeConfigIssue::new(
            field_path(node_id, "conditions"),
            format!("ROUTE node '{}' must define a non-empty conditions array", node_id),
        )),
        None => errors.push(NodeConfigIssue::new(
            field_path(node_id, "conditions"),
            format!("ROUTE node '{}' must define a non-empty conditions array", node_id),
        )),
        Some(conditions) => {
            for (idx, condition) in conditions.iter().enumerate() {
                if let Some(err) = require_string(
                    node_id,
                    node_type,
                    &Value::Object(condition.as_object().cloned().unwrap_or_default()),
                    "expression",
                    true,
                ) {
                    errors.push(NodeConfigIssue::new(
                        format!("nodes.{}.config.conditions[{}]", node_id, idx),
                        err.message,
                    ));
                }
                if let Some(err) = require_string(
                    node_id,
                    node_type,
                    &Value::Object(condition.as_object().cloned().unwrap_or_default()),
                    "target_node_id",
                    true,
                ) {
                    errors.push(NodeConfigIssue::new(
                        format!("nodes.{}.config.conditions[{}]", node_id, idx),
                        err.message,
                    ));
                }
            }
        }
    }
    errors
}

fn validate_fork_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("FORK node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    match config.get("fork_paths").and_then(|v| v.as_array()) {
        Some(arr) if arr.is_empty() => errors.push(NodeConfigIssue::new(
            field_path(node_id, "fork_paths"),
            format!("FORK node '{}' must define a non-empty fork_paths array", node_id),
        )),
        None => errors.push(NodeConfigIssue::new(
            field_path(node_id, "fork_paths"),
            format!("FORK node '{}' must define a non-empty fork_paths array", node_id),
        )),
        Some(paths) => {
            for (idx, path) in paths.iter().enumerate() {
                let path = path.as_object().cloned().unwrap_or_default();
                let path = Value::Object(path);
                if let Some(err) = require_string(node_id, node_type, &path, "path_id", true) {
                    errors.push(NodeConfigIssue::new(
                        format!("nodes.{}.config.fork_paths[{}]", node_id, idx),
                        err.message,
                    ));
                }
                if let Some(err) = require_string(node_id, node_type, &path, "child_node_id", true)
                {
                    errors.push(NodeConfigIssue::new(
                        format!("nodes.{}.config.fork_paths[{}]", node_id, idx),
                        err.message,
                    ));
                }
            }
        }
    }

    if let Some(err) = field_not_in(node_id, node_type, &config, "fork_strategy", &["serial", "parallel"])
    {
        errors.push(err);
    }
    errors
}

fn validate_join_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let mut errors = Vec::new();
    let Some(config) = config.and_then(|c| c.as_object()) else {
        errors.push(NodeConfigIssue::new(
            node_path(node_id),
            format!("JOIN node '{}' has no config", node_id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    match config.get("fork_path_ids").and_then(|v| v.as_array()) {
        Some(arr) if arr.is_empty() => errors.push(NodeConfigIssue::new(
            field_path(node_id, "fork_path_ids"),
            format!(
                "JOIN node '{}' must define a non-empty fork_path_ids array",
                node_id
            ),
        )),
        None => errors.push(NodeConfigIssue::new(
            field_path(node_id, "fork_path_ids"),
            format!(
                "JOIN node '{}' must define a non-empty fork_path_ids array",
                node_id
            ),
        )),
        Some(arr) => {
            let path_ids: Vec<&str> = arr
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .collect();
            if path_ids.len() != arr.len() {
                errors.push(NodeConfigIssue::new(
                    field_path(node_id, "fork_path_ids"),
                    format!("JOIN node '{}' has empty path ids", node_id),
                ));
            }
            if let Some(main) = config.get("main_path_id").and_then(|v| v.as_str()) {
                if !path_ids.contains(&main) {
                    errors.push(NodeConfigIssue::new(
                        field_path(node_id, "main_path_id"),
                        format!(
                            "JOIN node '{}' main_path_id '{}' is not in fork_path_ids",
                            node_id, main
                        ),
                    ));
                }
            }
        }
    }

    if let Some(err) = field_not_in(
        node_id,
        node_type,
        &config,
        "join_strategy",
        &["wait_for_all", "wait_for_any", "wait_for_n"],
    ) {
        errors.push(err);
    }
    errors
}

fn validate_subgraph_node(
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
        for (idx, input) in inputs.iter().enumerate() {
            let input = Value::Object(input.as_object().cloned().unwrap_or_default());
            if let Some(err) = require_string(node_id, node_type, &input, "source_path", true) {
                errors.push(NodeConfigIssue::new(
                    format!("nodes.{}.config.variable_inputs[{}]", node_id, idx),
                    err.message,
                ));
            }
            if let Some(err) = require_string(node_id, node_type, &input, "internal_name", true) {
                errors.push(NodeConfigIssue::new(
                    format!("nodes.{}.config.variable_inputs[{}]", node_id, idx),
                    err.message,
                ));
            }
        }
    }

    if let Some(outputs) = config.get("variable_outputs").and_then(|v| v.as_array()) {
        for (idx, output) in outputs.iter().enumerate() {
            let output = Value::Object(output.as_object().cloned().unwrap_or_default());
            if let Some(err) = require_string(node_id, node_type, &output, "internal_name", true) {
                errors.push(NodeConfigIssue::new(
                    format!("nodes.{}.config.variable_outputs[{}]", node_id, idx),
                    err.message,
                ));
            }
            if let Some(err) = require_string(node_id, node_type, &output, "target_path", true) {
                errors.push(NodeConfigIssue::new(
                    format!("nodes.{}.config.variable_outputs[{}]", node_id, idx),
                    err.message,
                ));
            }
        }
    }
    errors
}

fn validate_user_interaction_node(
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

fn validate_agent_loop_node(
    node_id: &str,
    _node_type: &str,
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
    let has_inline = config.get("inline_definition").filter(|v| !v.is_null()).is_some();
    if !has_loop_id && !has_inline {
        errors.push(NodeConfigIssue::new(
            format!("nodes.{}.config", node_id),
            format!(
                "AGENT_LOOP node '{}' must define agent_loop_id or inline_definition",
                node_id
            ),
        ));
    }
    errors
}

/// Validate the config of one workflow node by its node type. Unsupported
/// node types return no issues (their config invariants are checked by
/// graph-level validation instead).
///
/// `config` is the node's config value (`WorkflowNode.inner` or
/// `BaseStaticNode.config`). Node type names are case-insensitive.
pub fn validate_node_config(
    node_type: &str,
    node_id: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    // Field-required node types treat a missing config the same as an empty
    // object (so LLM without config reports a missing profile_id); the
    // container node types below report "has no config" instead.
    let empty = Value::Object(Default::default());
    let effective: Option<&Value> = match node_type.to_uppercase().as_str() {
        "LLM" | "SCRIPT" | "VARIABLE" => Some(config.unwrap_or(&empty)),
        _ => config,
    };

    match node_type.to_uppercase().as_str() {
        "LLM" => validate_llm_node(node_id, node_type, effective),
        "SCRIPT" => validate_script_node(node_id, node_type, effective),
        "VARIABLE" => validate_variable_node(node_id, node_type, effective),
        "ROUTE" => validate_route_node(node_id, node_type, effective),
        "FORK" => validate_fork_node(node_id, node_type, effective),
        "JOIN" => validate_join_node(node_id, node_type, effective),
        "SUBGRAPH" => validate_subgraph_node(node_id, node_type, effective),
        "USER_INTERACTION" => validate_user_interaction_node(node_id, node_type, effective),
        "AGENT_LOOP" => validate_agent_loop_node(node_id, node_type, effective),
        _ => Vec::new(),
    }
}

/// Validate every node's config and collect all issues.
pub fn validate_all_node_configs(nodes: &[(&str, &str, Option<&Value>)]) -> Vec<NodeConfigIssue> {
    let mut issues = Vec::new();
    for (node_id, node_type, config) in nodes {
        issues.extend(validate_node_config(node_type, node_id, *config));
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_requires_profile_id() {
        let missing = validate_node_config("LLM", "n1", Some(&serde_json::json!({})));
        assert_eq!(missing.len(), 1);
        assert!(missing[0].message.contains("profile_id"));

        let ok = validate_node_config(
            "LLM",
            "n1",
            Some(&serde_json::json!({"profile_id": "mock"})),
        );
        assert!(ok.is_empty());

        let camel = validate_node_config(
            "LLM",
            "n1",
            Some(&serde_json::json!({"profileId": "mock"})),
        );
        assert_eq!(camel.len(), 1, "camelCase profileId is not a canonical config");
    }

    #[test]
    fn variable_requires_name_and_expression() {
        let errors = validate_node_config("VARIABLE", "n1", Some(&serde_json::json!({})));
        assert_eq!(errors.len(), 2);

        let ok = validate_node_config(
            "VARIABLE",
            "n1",
            Some(&serde_json::json!({"variable_name": "x", "expression": "${input.a}"})),
        );
        assert!(ok.is_empty());

        let errors = validate_node_config(
            "VARIABLE",
            "n1",
            Some(&serde_json::json!({"variable_name": "x", "expression": "1", "variable_type": "nope"})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("invalid value"));
    }

    #[test]
    fn script_requires_name_and_valid_risk() {
        let errors = validate_node_config(
            "SCRIPT",
            "n1",
            Some(&serde_json::json!({"script_name": "s", "risk": "nope"})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("risk"));

        let ok = validate_node_config(
            "SCRIPT",
            "n1",
            Some(&serde_json::json!({"script_name": "s", "risk": "medium"})),
        );
        assert!(ok.is_empty());
    }

    #[test]
    fn fork_requires_branches() {
        assert_eq!(
            validate_node_config("FORK", "f", Some(&serde_json::json!({}))).len(),
            1
        );
        let errors =
            validate_node_config("FORK", "f", Some(&serde_json::json!({"fork_paths": []})));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("fork_paths"));

        let errors = validate_node_config(
            "FORK",
            "f",
            Some(&serde_json::json!({"fork_paths": [{"path_id": "p1"}]})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("child_node_id"));

        assert!(
            validate_node_config(
                "FORK",
                "f",
                Some(&serde_json::json!({"fork_paths": [{"path_id": "p1", "child_node_id": "n1"}]})),
            )
            .is_empty()
        );
    }

    #[test]
    fn join_requires_path_ids() {
        assert_eq!(
            validate_node_config("JOIN", "j", Some(&serde_json::json!({}))).len(),
            1
        );

        let errors = validate_node_config(
            "JOIN",
            "j",
            Some(&serde_json::json!({"fork_path_ids": ["p1"], "main_path_id": "p2"})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("main_path_id"));

        assert!(validate_node_config(
            "JOIN",
            "j",
            Some(&serde_json::json!({"fork_path_ids": ["p1"], "join_strategy": "wait_for_all"})),
        )
        .is_empty());

        let errors = validate_node_config(
            "JOIN",
            "j",
            Some(&serde_json::json!({"fork_path_ids": ["p1"], "join_strategy": "merge"})),
        );
        assert_eq!(errors.len(), 1, "legacy strategy values are not canonical");
    }

    #[test]
    fn subgraph_requires_id() {
        assert_eq!(
            validate_node_config("SUBGRAPH", "s", Some(&serde_json::json!({}))).len(),
            1
        );

        assert!(validate_node_config(
            "SUBGRAPH",
            "s",
            Some(&serde_json::json!({"subgraph_id": "wf-1"})),
        )
        .is_empty());

        let errors = validate_node_config(
            "SUBGRAPH",
            "s",
            Some(&serde_json::json!({
                "subgraph_id": "wf-1",
                "variable_inputs": [{"source_path": "a"}],
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("internal_name"));
    }

    #[test]
    fn user_interaction_requires_prompt_and_operation() {
        assert_eq!(
            validate_node_config("USER_INTERACTION", "u", Some(&serde_json::json!({}))).len(),
            2
        );

        let errors = validate_node_config(
            "USER_INTERACTION",
            "u",
            Some(&serde_json::json!({"prompt": "confirm?"})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("operation_type"));

        assert!(validate_node_config(
            "USER_INTERACTION",
            "u",
            Some(&serde_json::json!({"prompt": "confirm?", "operation_type": "update_variables"})),
        )
        .is_empty());
    }

    #[test]
    fn agent_loop_requires_definition() {
        assert_eq!(
            validate_node_config("AGENT_LOOP", "a", Some(&serde_json::json!({}))).len(),
            1
        );

        assert!(validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({"agent_loop_id": "loop-1"})),
        )
        .is_empty());

        assert!(validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({"inline_definition": {"id": "a1", "name": "agent"}})),
        )
        .is_empty());

        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({"model": "mock"})),
        );
        assert_eq!(errors.len(), 1, "top-level model is not a canonical AGENT_LOOP config");
    }

    #[test]
    fn route_requires_conditions() {
        assert_eq!(
            validate_node_config("ROUTE", "r", Some(&serde_json::json!({}))).len(),
            1
        );

        let errors = validate_node_config(
            "ROUTE",
            "r",
            Some(&serde_json::json!({"conditions": [{"expression": "${a} > 1"}]})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("target_node_id"));

        assert!(validate_node_config(
            "ROUTE",
            "r",
            Some(&serde_json::json!({"conditions": [{
                "expression": "${a} > 1",
                "target_node_id": "next"
            }]})),
        )
        .is_empty());
    }

    #[test]
    fn unknown_node_types_are_skipped() {
        assert!(validate_node_config("START", "s", Some(&serde_json::json!({}))).is_empty());
        assert!(validate_node_config("END", "e", Some(&serde_json::json!({}))).is_empty());
        assert!(validate_node_config("LOOP_START", "ls", Some(&serde_json::json!({}))).is_empty());
    }

    #[test]
    fn validate_all_collects_from_iterator() {
        let nodes: Vec<(&str, &str, Option<Value>)> = vec![
            ("n1", "LLM", Some(serde_json::json!({}))),
            ("n2", "SCRIPT", Some(serde_json::json!({"script_name": "s", "risk": "medium"}))),
        ];
        let tuples: Vec<(&str, &str, Option<&Value>)> = nodes
            .iter()
            .map(|(id, ty, cfg)| (*id, *ty, cfg.as_ref()))
            .collect();
        let issues = validate_all_node_configs(&tuples);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("profile_id"));
    }
}
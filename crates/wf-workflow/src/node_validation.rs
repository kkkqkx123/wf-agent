use serde_json::Value;
use wf_types::workflow_execution::{WorkflowGraphStructure, WorkflowNode};

use crate::validation::ValidationError;

fn require_string(
    node: &WorkflowNode,
    config: &Value,
    field: &str,
    required: bool,
) -> Option<ValidationError> {
    match config.get(field) {
        Some(Value::String(s)) if !s.trim().is_empty() => None,
        Some(_) => Some(ValidationError::new(
            format!("nodes.{}.config.{}", node.id, field),
            format!(
                "Node '{}' ({}) field '{}' must be a non-empty string",
                node.id, node.node_type, field
            ),
        )),
        None if required => Some(ValidationError::new(
            format!("nodes.{}.config.{}", node.id, field),
            format!(
                "Node '{}' ({}) is missing required config '{}'",
                node.id, node.node_type, field
            ),
        )),
        None => None,
    }
}

fn field_not_in(
    node: &WorkflowNode,
    config: &Value,
    field: &str,
    allowed: &[&str],
) -> Option<ValidationError> {
    let value = config.get(field)?;
    let actual = value.as_str().unwrap_or_default();
    if allowed.contains(&actual) {
        return None;
    }
    Some(ValidationError::new(
        format!("nodes.{}.config.{}", node.id, field),
        format!(
            "Node '{}' ({}) field '{}' has invalid value '{}', expected one of {}",
            node.id,
            node.node_type,
            field,
            actual,
            allowed.join(", ")
        ),
    ))
}

fn validate_llm_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if let Some(config) = node.inner.as_object() {
        if let Some(err) = require_string(node, &Value::Object(config.clone()), "profile_id", true)
        {
            errors.push(err);
        }
    }
    errors
}

fn validate_script_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if let Some(config) = node.inner.as_object() {
        let config = Value::Object(config.clone());
        if let Some(err) = require_string(node, &config, "script_name", true) {
            errors.push(err);
        }
        if let Some(err) = require_string(node, &config, "risk", true) {
            errors.push(err);
        }
        if let Some(err) = field_not_in(node, &config, "risk", &["none", "low", "medium", "high"]) {
            errors.push(err);
        }
    }
    errors
}

fn validate_variable_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if let Some(config) = node.inner.as_object() {
        let config = Value::Object(config.clone());
        if let Some(err) = require_string(node, &config, "variable_name", true) {
            errors.push(err);
        }
        if let Some(err) = require_string(node, &config, "expression", true) {
            errors.push(err);
        }
        if let Some(err) = field_not_in(
            node,
            &config,
            "variable_type",
            &["number", "string", "boolean", "array", "object"],
        ) {
            errors.push(err);
        }
        if let Some(err) = field_not_in(
            node,
            &config,
            "scope",
            &["global", "workflow_execution", "subgraph", "loop"],
        ) {
            errors.push(err);
        }
    }
    errors
}

fn validate_route_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let Some(config) = node.inner.as_object() else {
        errors.push(ValidationError::new(
            format!("nodes.{}", node.id),
            format!("ROUTE node '{}' has no config", node.id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    match config.get("conditions").and_then(|v| v.as_array()) {
        Some(arr) if arr.is_empty() => errors.push(ValidationError::new(
            format!("nodes.{}.config.conditions", node.id),
            format!(
                "ROUTE node '{}' must define a non-empty conditions array",
                node.id
            ),
        )),
        None => errors.push(ValidationError::new(
            format!("nodes.{}.config.conditions", node.id),
            format!(
                "ROUTE node '{}' must define a non-empty conditions array",
                node.id
            ),
        )),
        Some(conditions) => {
            for (idx, condition) in conditions.iter().enumerate() {
                if let Some(err) = require_string(
                    node,
                    &Value::Object(condition.as_object().cloned().unwrap_or_default()),
                    "expression",
                    true,
                ) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.conditions[{}]", node.id, idx),
                        err.message,
                    ));
                }
                if let Some(err) = require_string(
                    node,
                    &Value::Object(condition.as_object().cloned().unwrap_or_default()),
                    "target_node_id",
                    true,
                ) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.conditions[{}]", node.id, idx),
                        err.message,
                    ));
                }
            }
        }
    }
    errors
}

fn validate_fork_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let Some(config) = node.inner.as_object() else {
        errors.push(ValidationError::new(
            format!("nodes.{}", node.id),
            format!("FORK node '{}' has no config", node.id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    match config.get("fork_paths").and_then(|v| v.as_array()) {
        Some(arr) if arr.is_empty() => errors.push(ValidationError::new(
            format!("nodes.{}.config.fork_paths", node.id),
            format!(
                "FORK node '{}' must define a non-empty fork_paths array",
                node.id
            ),
        )),
        None => errors.push(ValidationError::new(
            format!("nodes.{}.config.fork_paths", node.id),
            format!(
                "FORK node '{}' must define a non-empty fork_paths array",
                node.id
            ),
        )),
        Some(paths) => {
            for (idx, path) in paths.iter().enumerate() {
                let path = path.as_object().cloned().unwrap_or_default();
                let path = Value::Object(path);
                if let Some(err) = require_string(node, &path, "path_id", true) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.fork_paths[{}]", node.id, idx),
                        err.message,
                    ));
                }
                if let Some(err) = require_string(node, &path, "child_node_id", true) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.fork_paths[{}]", node.id, idx),
                        err.message,
                    ));
                }
            }
        }
    }

    if let Some(err) = field_not_in(node, &config, "fork_strategy", &["serial", "parallel"]) {
        errors.push(err);
    }
    errors
}

fn validate_join_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let Some(config) = node.inner.as_object() else {
        errors.push(ValidationError::new(
            format!("nodes.{}", node.id),
            format!("JOIN node '{}' has no config", node.id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    match config.get("fork_path_ids").and_then(|v| v.as_array()) {
        Some(arr) if arr.is_empty() => errors.push(ValidationError::new(
            format!("nodes.{}.config.fork_path_ids", node.id),
            format!(
                "JOIN node '{}' must define a non-empty fork_path_ids array",
                node.id
            ),
        )),
        None => errors.push(ValidationError::new(
            format!("nodes.{}.config.fork_path_ids", node.id),
            format!(
                "JOIN node '{}' must define a non-empty fork_path_ids array",
                node.id
            ),
        )),
        Some(arr) => {
            let path_ids: Vec<&str> = arr
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .collect();
            if path_ids.len() != arr.len() {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.fork_path_ids", node.id),
                    format!("JOIN node '{}' has empty path ids", node.id),
                ));
            }
            if let Some(main) = config.get("main_path_id").and_then(|v| v.as_str()) {
                if !path_ids.contains(&main) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.main_path_id", node.id),
                        format!(
                            "JOIN node '{}' main_path_id '{}' is not in fork_path_ids",
                            node.id, main
                        ),
                    ));
                }
            }
        }
    }

    if let Some(err) = field_not_in(
        node,
        &config,
        "join_strategy",
        &["wait_for_all", "wait_for_any", "wait_for_n"],
    ) {
        errors.push(err);
    }
    errors
}

fn validate_subgraph_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let Some(config) = node.inner.as_object() else {
        errors.push(ValidationError::new(
            format!("nodes.{}", node.id),
            format!("SUBGRAPH node '{}' has no config", node.id),
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
        errors.push(ValidationError::new(
            format!("nodes.{}.config.subgraph_id", node.id),
            format!(
                "SUBGRAPH node '{}' must define a non-empty subgraph_id (or embed_id)",
                node.id
            ),
        ));
    }

    if let Some(inputs) = config.get("variable_inputs").and_then(|v| v.as_array()) {
        for (idx, input) in inputs.iter().enumerate() {
            let input = Value::Object(input.as_object().cloned().unwrap_or_default());
            if let Some(err) = require_string(node, &input, "source_path", true) {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.variable_inputs[{}]", node.id, idx),
                    err.message,
                ));
            }
            if let Some(err) = require_string(node, &input, "internal_name", true) {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.variable_inputs[{}]", node.id, idx),
                    err.message,
                ));
            }
        }
    }

    if let Some(outputs) = config.get("variable_outputs").and_then(|v| v.as_array()) {
        for (idx, output) in outputs.iter().enumerate() {
            let output = Value::Object(output.as_object().cloned().unwrap_or_default());
            if let Some(err) = require_string(node, &output, "internal_name", true) {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.variable_outputs[{}]", node.id, idx),
                    err.message,
                ));
            }
            if let Some(err) = require_string(node, &output, "target_path", true) {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.variable_outputs[{}]", node.id, idx),
                    err.message,
                ));
            }
        }
    }
    errors
}

fn validate_user_interaction_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let Some(config) = node.inner.as_object() else {
        errors.push(ValidationError::new(
            format!("nodes.{}", node.id),
            format!("USER_INTERACTION node '{}' has no config", node.id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    if let Some(err) = require_string(node, &config, "prompt", true) {
        errors.push(err);
    }
    if let Some(err) = require_string(node, &config, "operation_type", true) {
        errors.push(err);
    }
    if let Some(err) = field_not_in(
        node,
        &config,
        "operation_type",
        &["update_variables", "add_message"],
    ) {
        errors.push(err);
    }
    errors
}

fn validate_agent_loop_node(node: &WorkflowNode) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let Some(config) = node.inner.as_object() else {
        errors.push(ValidationError::new(
            format!("nodes.{}", node.id),
            format!("AGENT_LOOP node '{}' has no config", node.id),
        ));
        return errors;
    };
    let config = Value::Object(config.clone());

    let has_loop_id = config
        .get("agent_loop_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    let has_inline = config
        .get("inline_definition")
        .filter(|v| !v.is_null())
        .is_some();
    if !has_loop_id && !has_inline {
        errors.push(ValidationError::new(
            format!("nodes.{}.config", node.id),
            format!(
                "AGENT_LOOP node '{}' must define agent_loop_id or inline_definition",
                node.id
            ),
        ));
    }
    errors
}

/// Validate the config of each common node type in the graph. Unsupported
/// node types are skipped (their config is validated by graph-level checks).
pub fn validate_node_configs(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for node in &graph.nodes {
        match node.node_type.as_str() {
            "LLM" => errors.extend(validate_llm_node(node)),
            "SCRIPT" => errors.extend(validate_script_node(node)),
            "VARIABLE" => errors.extend(validate_variable_node(node)),
            "ROUTE" => errors.extend(validate_route_node(node)),
            "FORK" => errors.extend(validate_fork_node(node)),
            "JOIN" => errors.extend(validate_join_node(node)),
            "SUBGRAPH" => errors.extend(validate_subgraph_node(node)),
            "USER_INTERACTION" => errors.extend(validate_user_interaction_node(node)),
            "AGENT_LOOP" => errors.extend(validate_agent_loop_node(node)),
            _ => {}
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn node(id: &str, node_type: &str, inner: Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn graph_with(nodes: Vec<WorkflowNode>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges: vec![],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: None,
            end_node_ids: vec![],
        }
    }

    #[test]
    fn llm_requires_profile_id() {
        let g = graph_with(vec![node("n1", "LLM", serde_json::json!({}))]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("profile_id"));

        let g = graph_with(vec![node(
            "n1",
            "LLM",
            serde_json::json!({"profile_id": "mock"}),
        )]);
        assert!(validate_node_configs(&g).is_empty());

        let g = graph_with(vec![node(
            "n1",
            "LLM",
            serde_json::json!({"profileId": "mock"}),
        )]);
        assert_eq!(
            validate_node_configs(&g).len(),
            1,
            "camelCase profileId is not a canonical config"
        );
    }

    #[test]
    fn variable_requires_name_and_expression() {
        let g = graph_with(vec![node("n1", "VARIABLE", serde_json::json!({}))]);
        assert_eq!(validate_node_configs(&g).len(), 2);

        let g = graph_with(vec![node(
            "n1",
            "VARIABLE",
            serde_json::json!({"variable_name": "x", "expression": "${input.a}"}),
        )]);
        assert!(validate_node_configs(&g).is_empty());

        let g = graph_with(vec![node(
            "n1",
            "VARIABLE",
            serde_json::json!({"variable_name": "x", "expression": "1", "variable_type": "nope"}),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("invalid value"));

        let g = graph_with(vec![node(
            "n1",
            "VARIABLE",
            serde_json::json!({"variableName": "x", "expression": "1"}),
        )]);
        assert_eq!(
            validate_node_configs(&g).len(),
            1,
            "camelCase variableName is not a canonical config"
        );
    }

    #[test]
    fn script_requires_name_and_valid_risk() {
        let g = graph_with(vec![node(
            "n1",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "nope"}),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("risk"));

        let g = graph_with(vec![node(
            "n1",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        )]);
        assert!(validate_node_configs(&g).is_empty());
    }

    #[test]
    fn fork_requires_branches() {
        let g = graph_with(vec![node("f", "FORK", serde_json::json!({}))]);
        assert_eq!(validate_node_configs(&g).len(), 1);

        let g = graph_with(vec![node(
            "f",
            "FORK",
            serde_json::json!({"fork_paths": []}),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("fork_paths"));

        let g = graph_with(vec![node(
            "f",
            "FORK",
            serde_json::json!({"fork_paths": [{"path_id": "p1"}]}),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("child_node_id"));

        let g = graph_with(vec![node(
            "f",
            "FORK",
            serde_json::json!({"fork_paths": [{"path_id": "p1", "child_node_id": "n1"}]}),
        )]);
        assert!(validate_node_configs(&g).is_empty());

        let g = graph_with(vec![node(
            "f",
            "FORK",
            serde_json::json!({"branches": [{"id": "p1", "input": {}}]}),
        )]);
        assert_eq!(
            validate_node_configs(&g).len(),
            1,
            "legacy branches shape is not a canonical config"
        );
    }

    #[test]
    fn join_requires_path_ids() {
        let g = graph_with(vec![node("j", "JOIN", serde_json::json!({}))]);
        assert_eq!(validate_node_configs(&g).len(), 1);

        let g = graph_with(vec![node(
            "j",
            "JOIN",
            serde_json::json!({"fork_path_ids": ["p1"], "main_path_id": "p2"}),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("main_path_id"));

        let g = graph_with(vec![node(
            "j",
            "JOIN",
            serde_json::json!({"fork_path_ids": ["p1"], "join_strategy": "wait_for_all"}),
        )]);
        assert!(validate_node_configs(&g).is_empty());

        let g = graph_with(vec![node(
            "j",
            "JOIN",
            serde_json::json!({"fork_path_ids": ["p1"], "join_strategy": "merge"}),
        )]);
        assert_eq!(
            validate_node_configs(&g).len(),
            1,
            "legacy strategy values are not canonical"
        );
    }

    #[test]
    fn subgraph_requires_id() {
        let g = graph_with(vec![node("s", "SUBGRAPH", serde_json::json!({}))]);
        assert_eq!(validate_node_configs(&g).len(), 1);

        let g = graph_with(vec![node(
            "s",
            "SUBGRAPH",
            serde_json::json!({"subgraph_id": "wf-1"}),
        )]);
        assert!(validate_node_configs(&g).is_empty());

        let g = graph_with(vec![node(
            "s",
            "SUBGRAPH",
            serde_json::json!({
                "subgraph_id": "wf-1",
                "variable_inputs": [{"source_path": "a"}],
            }),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("internal_name"));
    }

    #[test]
    fn user_interaction_requires_prompt_and_operation() {
        let g = graph_with(vec![node("u", "USER_INTERACTION", serde_json::json!({}))]);
        assert_eq!(validate_node_configs(&g).len(), 2);

        let g = graph_with(vec![node(
            "u",
            "USER_INTERACTION",
            serde_json::json!({"prompt": "confirm?"}),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("operation_type"));

        let g = graph_with(vec![node(
            "u",
            "USER_INTERACTION",
            serde_json::json!({"prompt": "confirm?", "operation_type": "update_variables"}),
        )]);
        assert!(validate_node_configs(&g).is_empty());
    }

    #[test]
    fn agent_loop_requires_definition() {
        let g = graph_with(vec![node("a", "AGENT_LOOP", serde_json::json!({}))]);
        assert_eq!(validate_node_configs(&g).len(), 1);

        let g = graph_with(vec![node(
            "a",
            "AGENT_LOOP",
            serde_json::json!({"agent_loop_id": "loop-1"}),
        )]);
        assert!(validate_node_configs(&g).is_empty());

        let g = graph_with(vec![node(
            "a",
            "AGENT_LOOP",
            serde_json::json!({"inline_definition": {"id": "a1", "name": "agent"}}),
        )]);
        assert!(validate_node_configs(&g).is_empty());

        let g = graph_with(vec![node(
            "a",
            "AGENT_LOOP",
            serde_json::json!({"model": "mock"}),
        )]);
        assert_eq!(
            validate_node_configs(&g).len(),
            1,
            "top-level model is not a canonical AGENT_LOOP config"
        );
    }

    #[test]
    fn route_requires_conditions() {
        let g = graph_with(vec![node("r", "ROUTE", serde_json::json!({}))]);
        assert_eq!(validate_node_configs(&g).len(), 1);

        let g = graph_with(vec![node(
            "r",
            "ROUTE",
            serde_json::json!({"conditions": [{"expression": "${a} > 1"}]}),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("target_node_id"));

        let g = graph_with(vec![node(
            "r",
            "ROUTE",
            serde_json::json!({"conditions": [{
                "expression": "${a} > 1",
                "target_node_id": "next"
            }]}),
        )]);
        assert!(validate_node_configs(&g).is_empty());

        let g = graph_with(vec![node(
            "r",
            "ROUTE",
            serde_json::json!({"branches": [{"condition": "${a} > 1", "next_node": "next"}]}),
        )]);
        assert_eq!(
            validate_node_configs(&g).len(),
            1,
            "legacy branches shape is not a canonical ROUTE config"
        );
    }

    #[test]
    fn unknown_node_types_are_skipped() {
        let g = graph_with(vec![
            node("s", "START", serde_json::json!({})),
            node("e", "END", serde_json::json!({})),
            node("ls", "LOOP_START", serde_json::json!({})),
        ]);
        assert!(validate_node_configs(&g).is_empty());
    }
}

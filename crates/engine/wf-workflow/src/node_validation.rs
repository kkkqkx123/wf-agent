use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::validation::ValidationError;

/// Validate the config of each workflow node. Field-level business rules are
/// owned by the shared `wf-config` node validators (single source of truth,
/// also used by the wf-api save path); this adapter only bridges the graph
/// structure to the shared validator and re-shapes issues into graph-level
/// validation errors.
pub fn validate_node_configs(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for node in &graph.nodes {
        for issue in wf_config::processor::node_config::validate_node_config(
            &node.node_type,
            &node.id,
            Some(&node.inner),
        ) {
            errors.push(ValidationError::new(issue.field, issue.message));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;
    use wf_types::workflow_execution::WorkflowNode;

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
            serde_json::json!({"inline_definition": {"id": "a1", "name": "agent", "config": {"profile_id": "mock"}}}),
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
    fn known_types_without_field_validators_pass() {
        let g = graph_with(vec![
            node("s", "START", serde_json::json!({})),
            node("e", "END", serde_json::json!({})),
            node("ls", "LOOP_START", serde_json::json!({})),
        ]);
        assert!(validate_node_configs(&g).is_empty());
    }

    #[test]
    fn unknown_node_type_is_rejected() {
        let g = graph_with(vec![node("x", "LLMM", serde_json::json!({}))]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unknown node type"));
    }

    #[test]
    fn llm_typed_enhancement_fields_are_checked() {
        let g = graph_with(vec![node(
            "n1",
            "LLM",
            serde_json::json!({"profile_id": "mock", "violation_policy": "auto-convertt"}),
        )]);
        let errors = validate_node_configs(&g);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("violation_policy"));

        let g = graph_with(vec![node(
            "n1",
            "LLM",
            serde_json::json!({"profile_id": "mock", "violation_policy": "fail"}),
        )]);
        assert!(validate_node_configs(&g).is_empty());
    }
}

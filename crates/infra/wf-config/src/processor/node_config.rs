mod agent_loop;
mod common;
mod fork_join;
mod interaction;
mod llm;
mod route;
mod script;
mod subgraph;
mod variable;

pub use common::NodeConfigIssue;

use serde_json::Value;

use self::agent_loop::validate_agent_loop_node;
use self::fork_join::{validate_fork_node, validate_join_node};
use self::interaction::{validate_tool_visibility_node, validate_user_interaction_node};
use self::llm::validate_llm_node;
use self::route::validate_route_node;
use self::script::validate_script_node;
use self::subgraph::validate_subgraph_node;
use self::variable::validate_variable_node;

/// Validate the config of one workflow node by its node type.
///
/// Unknown node types are rejected with an issue: there is no fallback type.
/// `config` is the node's config value (`WorkflowNode.inner` or
/// `BaseStaticNode.config`). Node type names are case-insensitive.
pub fn validate_node_config(
    node_type: &str,
    node_id: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
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
        "TOOL_VISIBILITY" => validate_tool_visibility_node(node_id, node_type, effective),
        "START"
        | "END"
        | "EMBED_START"
        | "EMBED_END"
        | "SYNC"
        | "EMBED_GRAPH"
        | "INTERACTIVE_SCRIPT"
        | "CONTEXT_PROCESSOR"
        | "LOOP_START"
        | "LOOP_END"
        | "START_FROM_MESSAGE"
        | "CONTINUE_FROM_MESSAGE" => Vec::new(),
        _ => vec![NodeConfigIssue::new(
            common::node_path(node_id),
            format!("Node '{node_id}' has unknown node type '{node_type}'"),
        )],
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

        let camel =
            validate_node_config("LLM", "n1", Some(&serde_json::json!({"profileId": "mock"})));
        assert_eq!(
            camel.len(),
            1,
            "camelCase profileId is not a canonical config"
        );
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
            Some(
                &serde_json::json!({"variable_name": "x", "expression": "1", "variable_type": "nope"}),
            ),
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

        assert!(validate_node_config(
            "FORK",
            "f",
            Some(&serde_json::json!({"fork_paths": [{"path_id": "p1", "child_node_id": "n1"}]})),
        )
        .is_empty());
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
            Some(&serde_json::json!({"inline_definition": {"id": "a1", "name": "agent", "config": {"profile_id": "mock"}}})),
        )
        .is_empty());

        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({"inline_definition": {"id": "a1", "name": "agent"}})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("config"));

        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({"model": "mock"})),
        );
        assert_eq!(
            errors.len(),
            1,
            "top-level model is not a canonical AGENT_LOOP config"
        );
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
    fn known_types_without_field_validators_pass() {
        assert!(validate_node_config("START", "s", Some(&serde_json::json!({}))).is_empty());
        assert!(validate_node_config("END", "e", Some(&serde_json::json!({}))).is_empty());
        assert!(validate_node_config("LOOP_START", "ls", Some(&serde_json::json!({}))).is_empty());
    }

    #[test]
    fn unknown_node_type_is_rejected() {
        let errors = validate_node_config("LLMM", "n1", Some(&serde_json::json!({})));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unknown node type"));
    }

    #[test]
    fn llm_rejects_invalid_violation_policy() {
        let ok = validate_node_config(
            "LLM",
            "n1",
            Some(&serde_json::json!({"profile_id": "mock", "violation_policy": "auto_convert"})),
        );
        assert!(ok.is_empty());

        let errors = validate_node_config(
            "LLM",
            "n1",
            Some(&serde_json::json!({"profile_id": "mock", "violation_policy": "auto-convertt"})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("violation_policy"));
    }

    #[test]
    fn llm_rejects_invalid_typed_enhancement_fields() {
        let errors = validate_node_config(
            "LLM",
            "n1",
            Some(&serde_json::json!({
                "profile_id": "mock",
                "dead_loop_detection": {"enabled": "yes"},
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("dead_loop_detection"));

        let errors = validate_node_config(
            "LLM",
            "n1",
            Some(&serde_json::json!({
                "profile_id": "mock",
                "generation": {"temperature": "hot"},
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("generation"));

        let errors = validate_node_config(
            "LLM",
            "n1",
            Some(&serde_json::json!({
                "profile_id": "mock",
                "max_tool_calls_per_request": "many",
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("execution settings"));
    }

    #[test]
    fn agent_loop_validates_inline_agent_config() {
        let ok = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "agent",
                    "config": {"profile_id": "mock", "violation_policy": "fail"},
                },
            })),
        );
        assert!(ok.is_empty());

        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({"inline_definition": "a1"})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("inline_definition"));

        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "agent",
                    "config": {"violation_policy": "explod"},
                },
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("inline agent config"));
    }

    #[test]
    fn validate_all_collects_from_iterator() {
        let nodes: Vec<(&str, &str, Option<Value>)> = vec![
            ("n1", "LLM", Some(serde_json::json!({}))),
            (
                "n2",
                "SCRIPT",
                Some(serde_json::json!({"script_name": "s", "risk": "medium"})),
            ),
        ];
        let tuples: Vec<(&str, &str, Option<&Value>)> = nodes
            .iter()
            .map(|(id, ty, cfg)| (*id, *ty, cfg.as_ref()))
            .collect();
        let issues = validate_all_node_configs(&tuples);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("profile_id"));
    }

    #[test]
    fn variable_rejects_bad_names_and_paths() {
        assert!(validate_node_config(
            "VARIABLE",
            "n1",
            Some(&serde_json::json!({"variable_name": "__secret", "expression": "1"})),
        )
        .is_empty());

        let errors = validate_node_config(
            "VARIABLE",
            "n1",
            Some(&serde_json::json!({"variable_name": "9bad", "expression": "1"})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("variable_name"));

        let errors = validate_node_config(
            "VARIABLE",
            "n1",
            Some(&serde_json::json!({"variable_name": "x", "expression": "hello ${} world"})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].field.contains("expression"));

        let errors = validate_node_config(
            "VARIABLE",
            "n1",
            Some(
                &serde_json::json!({"variable_name": "x", "expression": "hello ${unclosed world"}),
            ),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unterminated"));

        assert!(validate_node_config(
            "VARIABLE",
            "n1",
            Some(&serde_json::json!({"variable_name": "x", "expression": "${user.name} done"})),
        )
        .is_empty());
    }

    #[test]
    fn route_accepts_default_only_and_rejects_bad_syntax() {
        assert!(validate_node_config(
            "ROUTE",
            "r",
            Some(&serde_json::json!({"default_target_node_id": "end"})),
        )
        .is_empty());

        let errors = validate_node_config(
            "ROUTE",
            "r",
            Some(&serde_json::json!({"conditions": [{
                "expression": "eq(a)",
                "target_node_id": "next"
            }]})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("condition expression"));

        let errors = validate_node_config(
            "ROUTE",
            "r",
            Some(&serde_json::json!({
                "conditions": [{"expression": "eq(a, 1)", "target_node_id": "end"}],
                "default_target_node_id": "end",
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicates the default"));

        let errors = validate_node_config(
            "ROUTE",
            "r",
            Some(&serde_json::json!({
                "conditions": [{"expression": "nope_fn(a, b)", "target_node_id": "next"}],
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("Unknown condition function"));
    }

    #[test]
    fn tool_visibility_shape_is_checked() {
        assert!(validate_node_config(
            "TOOL_VISIBILITY",
            "t",
            Some(&serde_json::json!({"action": "block", "tool_ids": ["shell"]})),
        )
        .is_empty());
        assert!(validate_node_config(
            "TOOL_VISIBILITY",
            "t",
            Some(&serde_json::json!({"tool_ids": ["shell"]})),
        )
        .is_empty());

        let errors = validate_node_config(
            "TOOL_VISIBILITY",
            "t",
            Some(&serde_json::json!({"action": "hide", "tool_ids": ["shell"]})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("action"));

        let errors = validate_node_config(
            "TOOL_VISIBILITY",
            "t",
            Some(&serde_json::json!({"action": "block", "tool_ids": []})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("tool_ids"));

        let errors = validate_node_config(
            "TOOL_VISIBILITY",
            "t",
            Some(&serde_json::json!({"action": "block", "tool_ids": ["a", "a"]})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("more than once"));

        let errors = validate_node_config(
            "TOOL_VISIBILITY",
            "t",
            Some(&serde_json::json!({"action": "block", "tool_ids": [""]})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].field.contains("tool_ids"));
    }

    #[test]
    fn fork_rejects_duplicate_and_invalid_path_ids() {
        let errors = validate_node_config(
            "FORK",
            "f",
            Some(&serde_json::json!({"fork_paths": [
                {"path_id": "p1", "child_node_id": "a"},
                {"path_id": "p1", "child_node_id": "b"},
            ]})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate"));

        let errors = validate_node_config(
            "FORK",
            "f",
            Some(&serde_json::json!({"fork_paths": [
                {"path_id": "9bad", "child_node_id": "a"},
            ]})),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("invalid path_id"));
    }

    #[test]
    fn join_rejects_duplicate_and_bad_threshold() {
        let errors = validate_node_config(
            "JOIN",
            "j",
            Some(&serde_json::json!({"fork_path_ids": ["p1", "p1"]})),
        );
        assert!(errors.iter().any(|e| e.message.contains("duplicate")));

        let errors = validate_node_config(
            "JOIN",
            "j",
            Some(&serde_json::json!({
                "fork_path_ids": ["p1", "p2"],
                "join_strategy": "wait_for_n",
                "threshold": 5,
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("threshold"));

        let errors = validate_node_config(
            "JOIN",
            "j",
            Some(&serde_json::json!({
                "fork_path_ids": ["p1"],
                "join_strategy": "wait_for_n",
                "threshold": 0,
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("threshold"));

        assert!(validate_node_config(
            "JOIN",
            "j",
            Some(&serde_json::json!({
                "fork_path_ids": ["p1", "p2"],
                "join_strategy": "wait_for_n",
                "threshold": 1,
            })),
        )
        .is_empty());
    }

    #[test]
    fn subgraph_rejects_bad_mapping_format() {
        let errors = validate_node_config(
            "SUBGRAPH",
            "s",
            Some(&serde_json::json!({
                "subgraph_id": "wf-1",
                "variable_inputs": [{"source_path": "a..b", "internal_name": "x"}],
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("source_path"));

        let errors = validate_node_config(
            "SUBGRAPH",
            "s",
            Some(&serde_json::json!({
                "subgraph_id": "wf-1",
                "variable_inputs": [
                    {"source_path": "a", "internal_name": "x"},
                    {"source_path": "b", "internal_name": "x"},
                ],
            })),
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("more than once"));

        assert!(validate_node_config(
            "SUBGRAPH",
            "s",
            Some(&serde_json::json!({
                "subgraph_id": "wf-1",
                "variable_inputs": [{"source_path": "a", "internal_name": "__x"}],
            })),
        )
        .is_empty());
        assert!(validate_node_config(
            "SUBGRAPH",
            "s",
            Some(&serde_json::json!({
                "subgraph_id": "wf-1",
                "variable_outputs": [{"internal_name": "x", "target_path": "__secret"}],
            })),
        )
        .is_empty());
    }

    #[test]
    fn agent_loop_inline_requires_profile_and_ranges() {
        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "agent",
                    "config": {"profile_id": "", "max_iterations": 5},
                },
            })),
        );
        assert!(errors.iter().any(|e| e.message.contains("profile_id")));

        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "agent",
                    "config": {"profile_id": "mock", "max_iterations": 0},
                },
            })),
        );
        assert!(errors.iter().any(|e| e.message.contains("max_iterations")));

        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "agent",
                    "config": {"profile_id": "mock", "tool_call_format": "yaml"},
                },
            })),
        );
        assert!(errors
            .iter()
            .any(|e| e.message.contains("tool_call_format")));

        let errors = validate_node_config(
            "AGENT_LOOP",
            "a",
            Some(&serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "agent",
                    "config": {
                        "profile_id": "mock",
                        "available_tools": {"available": [""]},
                    },
                },
            })),
        );
        assert!(errors.iter().any(|e| e.message.contains("tool name")));
    }
}

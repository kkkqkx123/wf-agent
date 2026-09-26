//! Unit tests for the graph validation rule modules.

use std::collections::HashMap;

use wf_types::workflow::edge::EdgeType;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

use super::*;

fn make_node(id: &str, node_type: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner: serde_json::json!({}),
    }
}

fn make_node_with_inner(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner,
    }
}

fn make_edge(id: &str, source: &str, target: &str) -> WorkflowEdge {
    WorkflowEdge {
        id: id.to_string(),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Default,
        condition: None,
        label: None,
        description: None,
        error_route: None,
    }
}

fn make_graph(
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
    start: Option<&str>,
    ends: Vec<&str>,
) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: start.map(|s| s.to_string()),
        end_node_ids: ends.into_iter().map(|s| s.to_string()).collect(),
        error_default: None,
    }
}

#[test]
fn test_valid_linear_graph() {
    let graph = make_graph(
        vec![make_node("start", "START"), make_node("end", "END")],
        vec![make_edge("e1", "start", "end")],
        Some("start"),
        vec!["end"],
    );
    assert!(GraphValidator::validate(graph).is_ok());
}

#[test]
fn test_empty_nodes() {
    let graph = make_graph(vec![], vec![], None, vec![]);
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("at least one node")));
}

#[test]
fn test_duplicate_node_id() {
    let graph = make_graph(
        vec![make_node("n1", "START"), make_node("n1", "END")],
        vec![make_edge("e1", "n1", "n1")],
        Some("n1"),
        vec!["n1"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("Duplicate")));
}

#[test]
fn test_missing_start() {
    let graph = make_graph(
        vec![make_node("n1", "VARIABLE")],
        vec![],
        Some("n1"),
        vec!["n1"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("START")));
}

#[test]
fn test_missing_edge_target() {
    let graph = make_graph(
        vec![make_node("start", "START"), make_node("end", "END")],
        vec![make_edge("e1", "start", "nonexistent")],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("not found")));
}

#[test]
fn test_trigger_graph() {
    let graph = make_graph(
        vec![
            make_node("message_start", "START_FROM_MESSAGE"),
            make_node("message_end", "CONTINUE_FROM_MESSAGE"),
        ],
        vec![make_edge("e1", "message_start", "message_end")],
        None,
        vec![],
    );
    assert!(GraphValidator::validate(graph).is_ok());
}

#[test]
fn test_trigger_graph_with_start() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node("message_start", "START_FROM_MESSAGE"),
            make_node("message_end", "CONTINUE_FROM_MESSAGE"),
        ],
        vec![],
        None,
        vec![],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("cannot contain START")));
}

#[test]
fn test_fork_with_empty_branches() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner("fork", "FORK", serde_json::json!({"fork_paths": []})),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "fork"),
            make_edge("e2", "fork", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("empty fork_paths")));
}

#[test]
fn test_isolated_node_detected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node("v1", "VARIABLE"),
            make_node("end", "END"),
        ],
        vec![make_edge("e1", "start", "end")],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("isolated")));
}

#[test]
fn test_start_cannot_have_incoming_edges() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node("v1", "VARIABLE"),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "v1"),
            make_edge("e2", "v1", "end"),
            make_edge("e3", "v1", "start"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("START node cannot have incoming edges")));
}

#[test]
fn test_end_cannot_have_outgoing_edges() {
    let graph = make_graph(
        vec![make_node("start", "START"), make_node("end", "END")],
        vec![
            make_edge("e1", "start", "end"),
            make_edge("e2", "end", "start"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("END node (end) cannot have outgoing edges")));
}

#[test]
fn test_cycle_detected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node("a", "VARIABLE"),
            make_node("b", "VARIABLE"),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "a"),
            make_edge("e2", "a", "b"),
            make_edge("e3", "b", "a"),
            make_edge("e4", "b", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("Circular dependencies")));
}

#[test]
fn test_loop_pair_is_valid() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner("ls", "LOOP_START", serde_json::json!({"loop_id": "l1"})),
            make_node_with_inner(
                "body",
                "VARIABLE",
                serde_json::json!({"variable_name": "body", "expression": "1"}),
            ),
            make_node_with_inner(
                "le",
                "LOOP_END",
                serde_json::json!({"loop_id": "l1", "loop_start_node_id": "ls"}),
            ),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "ls"),
            make_edge("e2", "ls", "body"),
            make_edge("e3", "body", "le"),
            make_edge("e4", "le", "ls"),
            make_edge("e5", "le", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    assert!(GraphValidator::validate(graph).is_ok());
}

#[test]
fn test_unpaired_loop_end_detected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner("ls", "LOOP_START", serde_json::json!({"loop_id": "l1"})),
            make_node("end", "END"),
        ],
        vec![make_edge("e1", "start", "ls"), make_edge("e2", "ls", "end")],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("no matching LOOP_END")));
}

#[test]
fn test_fork_join_pairing_ok() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [
                        {"path_id": "p1", "child_node_id": "a1"},
                        {"path_id": "p2", "child_node_id": "a2"}
                    ]
                }),
            ),
            make_node_with_inner(
                "a1",
                "VARIABLE",
                serde_json::json!({"variable_name": "a1", "expression": "1"}),
            ),
            make_node_with_inner(
                "a2",
                "VARIABLE",
                serde_json::json!({"variable_name": "a2", "expression": "2"}),
            ),
            make_node_with_inner(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"]}),
            ),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "fork"),
            make_edge("e2", "fork", "a1"),
            make_edge("e3", "fork", "a2"),
            make_edge("e4", "a1", "join"),
            make_edge("e5", "a2", "join"),
            make_edge("e6", "join", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    assert!(GraphValidator::validate(graph).is_ok());
}

#[test]
fn test_fork_without_join_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [{"path_id": "p1", "child_node_id": "a1"}]
                }),
            ),
            make_node("a1", "VARIABLE"),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "fork"),
            make_edge("e2", "fork", "a1"),
            make_edge("e3", "a1", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("FORK node (fork) has no matching JOIN node")));
}

#[test]
fn test_fork_join_path_mismatch_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [
                        {"path_id": "p1", "child_node_id": "a1"},
                        {"path_id": "p2", "child_node_id": "a2"}
                    ]
                }),
            ),
            make_node("a1", "VARIABLE"),
            make_node("a2", "VARIABLE"),
            make_node_with_inner(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p3"]}),
            ),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "fork"),
            make_edge("e2", "fork", "a1"),
            make_edge("e3", "fork", "a2"),
            make_edge("e4", "a1", "join"),
            make_edge("e5", "a2", "join"),
            make_edge("e6", "join", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("do not match")));
}

#[test]
fn test_fork_join_unreachable_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [{"path_id": "p1", "child_node_id": "a1"}]
                }),
            ),
            make_node("a1", "VARIABLE"),
            make_node_with_inner("join", "JOIN", serde_json::json!({"fork_path_ids": ["p1"]})),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "fork"),
            make_edge("e2", "fork", "a1"),
            // a1 never connects to join: join is fed by nothing.
            make_edge("e3", "join", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("cannot reach the paired JOIN node")));
}

#[test]
fn test_sync_with_invalid_path_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [{"path_id": "p1", "child_node_id": "a1"}]
                }),
            ),
            make_node("a1", "VARIABLE"),
            make_node_with_inner("sync", "SYNC", serde_json::json!({"source_path_id": "pX"})),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "fork"),
            make_edge("e2", "fork", "a1"),
            make_edge("e3", "a1", "sync"),
            make_edge("e4", "sync", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("does not exist in any FORK node's fork_paths")));
}

#[test]
fn test_embed_graph_requires_definition() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node("embed", "EMBED_GRAPH"),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "embed"),
            make_edge("e2", "embed", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("missing embed_id configuration")));
}

#[test]
fn test_embed_graph_with_inline_definition_passes_config() {
    let inline = make_graph(
        vec![make_node("s2", "START"), make_node("e2", "END")],
        vec![make_edge("e2-1", "s2", "e2")],
        Some("s2"),
        vec!["e2"],
    );
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "embed",
                "EMBED_GRAPH",
                serde_json::json!({"graph_definition": inline}),
            ),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "embed"),
            make_edge("e2", "embed", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    assert!(
        GraphValidator::validate(graph).is_ok(),
        "inline graph definition must satisfy the embed requirement"
    );
}

#[test]
fn test_triggered_subgraph_disconnected_detected() {
    let graph = make_graph(
        vec![
            make_node("ts", "START_FROM_MESSAGE"),
            make_node("v1", "VARIABLE"),
            make_node("te", "CONTINUE_FROM_MESSAGE"),
        ],
        vec![make_edge("e1", "ts", "te")],
        None,
        vec![],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("not reachable from START_FROM_MESSAGE")));
}

#[test]
fn test_unreachable_node_detected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node("a", "VARIABLE"),
            make_node("end", "END"),
        ],
        vec![make_edge("e1", "start", "end")],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("is not reachable from START")));
}

#[test]
fn test_llm_node_missing_profile_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node("llm", "LLM"),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "llm"),
            make_edge("e2", "llm", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("missing required config 'profile_id'")));
}

#[test]
fn test_inconsistent_tool_call_protocols_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "l1",
                "LLM",
                serde_json::json!({
                    "profile_id": "mock",
                    "tool_call_protocol": "native",
                }),
            ),
            make_node_with_inner(
                "l2",
                "LLM",
                serde_json::json!({
                    "profile_id": "mock",
                    "tool_call_protocol": "xml",
                }),
            ),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "l1"),
            make_edge("e2", "l1", "l2"),
            make_edge("e3", "l2", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("Inconsistent tool call protocols")));
}

#[test]
fn test_route_with_invalid_expression_syntax_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "route",
                "ROUTE",
                serde_json::json!({
                    "conditions": [{"expression": "eq(only_one)", "target_node_id": "a"}],
                    "default_target_node_id": "end",
                }),
            ),
            make_node_with_inner(
                "a",
                "VARIABLE",
                serde_json::json!({"variable_name": "x", "expression": "1"}),
            ),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "route"),
            make_edge("e2", "route", "a"),
            make_edge("e3", "route", "end"),
            make_edge("e4", "a", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("invalid condition expression")));
}

#[test]
fn test_route_without_conditions_or_default_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner("route", "ROUTE", serde_json::json!({})),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "route"),
            make_edge("e2", "route", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e
        .message
        .contains("at least one condition or a default_target_node_id")));
}

#[test]
fn test_route_duplicate_default_target_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "route",
                "ROUTE",
                serde_json::json!({
                    "conditions": [{"expression": "eq(a, 1)", "target_node_id": "end"}],
                    "default_target_node_id": "end",
                }),
            ),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "route"),
            make_edge("e2", "route", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("duplicates the default target")));
}

#[test]
fn test_route_with_default_only_passes() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "route",
                "ROUTE",
                serde_json::json!({"default_target_node_id": "end"}),
            ),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "route"),
            make_edge("e2", "route", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    assert!(GraphValidator::validate(graph).is_ok());
}

#[test]
fn test_fork_with_empty_path_id_rejected() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [{"path_id": "", "child_node_id": "a1"}]
                }),
            ),
            make_node_with_inner(
                "a1",
                "VARIABLE",
                serde_json::json!({"variable_name": "x", "expression": "1"}),
            ),
            make_node_with_inner("join", "JOIN", serde_json::json!({"fork_path_ids": ["p1"]})),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "fork"),
            make_edge("e2", "fork", "a1"),
            make_edge("e3", "a1", "join"),
            make_edge("e4", "join", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("empty path_id")));
}

#[test]
fn test_fork_join_count_mismatch_reports_counts() {
    let graph = make_graph(
        vec![
            make_node("start", "START"),
            make_node_with_inner(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [
                        {"path_id": "p1", "child_node_id": "a1"},
                        {"path_id": "p2", "child_node_id": "a2"}
                    ]
                }),
            ),
            make_node_with_inner(
                "a1",
                "VARIABLE",
                serde_json::json!({"variable_name": "x", "expression": "1"}),
            ),
            make_node_with_inner(
                "a2",
                "VARIABLE",
                serde_json::json!({"variable_name": "y", "expression": "2"}),
            ),
            make_node_with_inner("join", "JOIN", serde_json::json!({"fork_path_ids": ["p1"]})),
            make_node("end", "END"),
        ],
        vec![
            make_edge("e1", "start", "fork"),
            make_edge("e2", "fork", "a1"),
            make_edge("e3", "fork", "a2"),
            make_edge("e4", "a1", "join"),
            make_edge("e5", "a2", "join"),
            make_edge("e6", "join", "end"),
        ],
        Some("start"),
        vec!["end"],
    );
    let result = GraphValidator::validate(graph);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("do not match") && e.message.contains("2 path(s)")));
}

#[test]
fn test_format_validation_report_lists_each_finding() {
    let errors = vec![
        ValidationError::new("nodes.a", "first problem"),
        ValidationError::new("nodes.b.config.x", "second problem"),
    ];
    let report = format_validation_report(&errors);
    assert!(report.contains("2 error(s) found:"));
    assert!(report.contains("[nodes.a] first problem"));
    assert!(report.contains("[nodes.b.config.x] second problem"));

    let empty = format_validation_report(&[]);
    assert!(empty.contains("0 error(s) found:"));
}

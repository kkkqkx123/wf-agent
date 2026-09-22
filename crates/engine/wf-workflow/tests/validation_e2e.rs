//! Integration tests for graph validation only (no execution).
//! Exercises `GraphValidator` end to end: a valid graph passes while
//! structural defects are reported with field paths.

use std::collections::HashMap;

use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};
use wf_workflow::{format_validation_report, GraphValidator};

fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner,
    }
}

fn edge(source: &str, target: &str) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{}-{}", source, target),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Default,
        condition: None,
        label: None,
        description: None,
    }
}

fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
    }
}

fn linear_graph() -> WorkflowGraphStructure {
    graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "x", "expression": "1"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "v1"), edge("v1", "end")],
    )
}

#[test]
fn valid_linear_graph_passes_validation() {
    let validated = GraphValidator::validate(linear_graph());
    assert!(validated.is_ok(), "valid graph must pass: {:?}", validated.err());
}

#[test]
fn graph_without_start_node_fails() {
    let mut g = linear_graph();
    g.nodes.retain(|n| n.node_type != "START");
    g.start_node_id = None;
    let err = GraphValidator::validate(g).expect_err("missing START must fail");
    assert!(!err.is_empty());
    let report = format_validation_report(&err);
    assert!(report.contains("error(s) found"));
}

#[test]
fn route_without_targets_fails() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node("route", "ROUTE", serde_json::json!({})),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "route"), edge("route", "end")],
    );
    let err = GraphValidator::validate(g).expect_err("bare ROUTE must fail");
    assert!(
        err.iter().any(|e| e.field.contains("route")),
        "route error must point at the node: {err:?}"
    );
}

#[test]
fn edge_to_unknown_node_fails() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node("end", "END", serde_json::json!({})),
        ],
        vec![WorkflowEdge {
            id: "start-ghost".to_string(),
            source_node_id: "start".to_string(),
            target_node_id: "ghost".to_string(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
        }],
    );
    let err = GraphValidator::validate(g).expect_err("dangling edge must fail");
    assert!(!err.is_empty());
}

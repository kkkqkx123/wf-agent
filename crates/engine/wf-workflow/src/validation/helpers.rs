//! Shared helpers for graph-level validation rule modules.

use std::collections::HashMap;

use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

pub(super) fn node_type_of<'a>(
    graph: &'a WorkflowGraphStructure,
    node_id: &str,
) -> Option<&'a str> {
    graph
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .map(|n| n.node_type.as_str())
}

pub(super) fn outgoing_edges<'a>(
    graph: &'a WorkflowGraphStructure,
    node_id: &str,
) -> Vec<&'a WorkflowEdge> {
    graph
        .edges
        .iter()
        .filter(|e| e.source_node_id == node_id)
        .collect()
}

pub(super) fn incoming_edges<'a>(
    graph: &'a WorkflowGraphStructure,
    node_id: &str,
) -> Vec<&'a WorkflowEdge> {
    graph
        .edges
        .iter()
        .filter(|e| e.target_node_id == node_id)
        .collect()
}

/// Extract the path ids of a FORK node config (`fork_paths[].path_id`).
pub(super) fn fork_path_ids(node: &WorkflowNode) -> Vec<String> {
    node.inner
        .get("fork_paths")
        .and_then(|v| v.as_array())
        .map(|paths| {
            paths
                .iter()
                .filter_map(|p| p.get("path_id").and_then(|v| v.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Join node config path ids (`fork_path_ids`).
pub(super) fn join_path_ids(node: &WorkflowNode) -> Vec<String> {
    node.inner
        .get("fork_path_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Node-id -> node-type lookup used by boundary checks.
pub(super) fn node_type_map(graph: &WorkflowGraphStructure) -> HashMap<&str, &str> {
    graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.node_type.as_str()))
        .collect()
}

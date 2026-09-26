//! Graph connectivity rules: triggered-subgraph internal reachability,
//! cycle rejection, and START/END reachability for normal workflows.

use wf_types::workflow_execution::WorkflowGraphStructure;
use wf_types::ValidationError;

use crate::analysis::{analyze_reachability, detect_cycles, get_reachable_nodes};

/// Internal connectivity of triggered subgraphs: every node must be
/// reachable from START_FROM_MESSAGE and able to reach
/// CONTINUE_FROM_MESSAGE.
pub(super) fn validate_triggered_subgraph(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let start_id = graph
        .nodes
        .iter()
        .find(|n| n.node_type == "START_FROM_MESSAGE")
        .map(|n| n.id.clone());
    let end_id = graph
        .nodes
        .iter()
        .find(|n| n.node_type == "CONTINUE_FROM_MESSAGE")
        .map(|n| n.id.clone());

    let (Some(start_id), Some(end_id)) = (start_id, end_id) else {
        return errors;
    };

    let reachable_from_start = get_reachable_nodes(graph, &start_id);
    for node in &graph.nodes {
        if node.node_type == "START_FROM_MESSAGE" {
            continue;
        }
        if !reachable_from_start.contains(&node.id) {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!(
                    "Node '{}' is not reachable from START_FROM_MESSAGE",
                    node.id
                ),
            ));
        }
    }

    for node in &graph.nodes {
        if node.node_type == "CONTINUE_FROM_MESSAGE" {
            continue;
        }
        if !reachable_from_start.contains(&node.id) {
            continue;
        }
        let reachable = get_reachable_nodes(graph, &node.id);
        if !reachable.contains(&end_id) {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!("Node '{}' cannot reach CONTINUE_FROM_MESSAGE", node.id),
            ));
        }
    }

    errors
}

/// Structural cycles are rejected. Loop continuation edges
/// (LOOP_END -> LOOP_START) are legal control flow and excluded.
pub(super) fn validate_cycles(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let result = detect_cycles(graph);
    if result.has_cycle {
        vec![ValidationError::new(
            "nodes",
            format!(
                "Circular dependencies exist in the workflow: cycle through nodes [{}]",
                result.cycle_nodes.join(", ")
            ),
        )]
    } else {
        Vec::new()
    }
}

/// Reachability for normal workflows: every node must be reachable from
/// START and must reach an END node. Triggered subgraphs use their own
/// connectivity validation instead.
pub(super) fn validate_reachability(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let has_trigger = graph
        .nodes
        .iter()
        .any(|n| n.node_type == "START_FROM_MESSAGE" || n.node_type == "CONTINUE_FROM_MESSAGE");
    if has_trigger {
        return Vec::new();
    }

    let mut errors = Vec::new();
    let analysis = analyze_reachability(graph);
    // ERROR edges count as reachability paths, so handlers entered only
    // through an error jump are already covered. The workflow catch-all
    // is not an edge: its dedicated target may legitimately have no
    // incoming edge at all.
    let default_target = graph
        .error_default
        .as_ref()
        .map(|d| d.target_node_id.as_str());

    for node_id in &analysis.unreachable_nodes {
        if default_target == Some(node_id.as_str()) {
            continue;
        }
        errors.push(ValidationError::new(
            format!("nodes.{}", node_id),
            format!("Node ({}) is not reachable from START node", node_id),
        ));
    }
    for node_id in &analysis.dead_end_nodes {
        errors.push(ValidationError::new(
            format!("nodes.{}", node_id),
            format!("Node ({}) cannot reach END node", node_id),
        ));
    }

    errors
}

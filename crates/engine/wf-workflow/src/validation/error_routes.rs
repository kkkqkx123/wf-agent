//! Error-routing rules: ERROR edge declarations, workflow catch-all
//! default, fork-subtree warnings, and error-route cycle detection.

use std::collections::HashSet;

use wf_types::workflow::edge::EdgeType;
use wf_types::workflow_execution::WorkflowGraphStructure;
use wf_types::ValidationError;

use crate::analysis::{detect_cycles, get_nodes_reaching_to, get_reachable_nodes};

/// Error-routing declarations ride on first-class ERROR edges plus the
/// workflow catch-all default. An `error_route` config on a non-ERROR
/// edge and a self-target jump are rejected; unknown edge endpoints are
/// already caught by the generic edge-reference check, so only the
/// default target needs a dedicated lookup here. Error jumps must not
/// introduce a control-flow cycle the normal edges alone do not have
/// (a handler re-entering an ancestor would loop between failing
/// nodes; the engine only bounds that at runtime, so reject it here).
/// The workflow default is excluded from the cycle check: it is an
/// explicit top-level escape hatch, not an edge a graph author wires.
/// Routing declared inside a FORK..JOIN span is warn-only: an isolated
/// error branch cannot safely merge back while sibling paths of the same
/// fork are still in flight.
pub(super) fn validate_error_branches(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();

    for edge in &graph.edges {
        let is_error = edge.r#type == EdgeType::Error;
        if !is_error && edge.error_route.is_some() {
            errors.push(ValidationError::new(
                format!("edges.{}", edge.id),
                "error_route config is only allowed on ERROR edges".to_string(),
            ));
        }
        if is_error && edge.source_node_id == edge.target_node_id {
            errors.push(ValidationError::new(
                format!("edges.{}", edge.id),
                format!(
                    "error route of node '{}' targets itself: the failed node never re-runs through its own route",
                    edge.source_node_id
                ),
            ));
        }
    }

    if let Some(default) = &graph.error_default {
        if !node_ids.contains(default.target_node_id.as_str()) {
            errors.push(ValidationError::new(
                "error_default",
                format!(
                    "workflow error default targets unknown node '{}'",
                    default.target_node_id
                ),
            ));
        }
    }

    warn_error_routes_in_fork_subtrees(graph);

    if errors.is_empty() {
        if let Some(cycle) = detect_error_route_cycle(graph) {
            errors.push(ValidationError::new(
                "nodes",
                format!(
                    "Error-route control cycle exists through nodes [{}]: an error jump must not re-enter an ancestor",
                    cycle.join(", ")
                ),
            ));
        }
    }
    errors
}

/// Warn (without failing validation) about ERROR edges declared inside
/// any FORK..JOIN span: nodes downstream of a FORK that still lead to a
/// JOIN through normal edges.
fn warn_error_routes_in_fork_subtrees(graph: &WorkflowGraphStructure) {
    let normal_only = WorkflowGraphStructure {
        nodes: graph.nodes.clone(),
        edges: graph
            .edges
            .iter()
            .filter(|e| e.r#type != EdgeType::Error)
            .cloned()
            .collect(),
        adjacency_list: Default::default(),
        reverse_adjacency_list: Default::default(),
        start_node_id: graph.start_node_id.clone(),
        end_node_ids: graph.end_node_ids.clone(),
        error_default: None,
    };
    let mut subtree: HashSet<String> = HashSet::new();
    for fork in normal_only.nodes.iter().filter(|n| n.node_type == "FORK") {
        let downstream = get_reachable_nodes(&normal_only, &fork.id);
        for join in normal_only.nodes.iter().filter(|n| n.node_type == "JOIN") {
            let up = get_nodes_reaching_to(&normal_only, &join.id);
            for id in downstream.iter().filter(|id| up.contains(*id)) {
                subtree.insert(id.clone());
            }
        }
    }
    for edge in &graph.edges {
        if edge.r#type == EdgeType::Error && subtree.contains(&edge.source_node_id) {
            tracing::warn!(
                edge_id = edge.id,
                source_node_id = edge.source_node_id,
                "error route declared inside a FORK..JOIN span: the isolated branch cannot safely merge while sibling paths are in flight"
            );
        }
    }
}

/// Detect a cycle that exists only once ERROR edges are treated as
/// control edges. Returns the cycle node ids, or `None` when the
/// normal+error graph is acyclic (or a normal-edge cycle already exists
/// and is reported elsewhere).
fn detect_error_route_cycle(graph: &WorkflowGraphStructure) -> Option<Vec<String>> {
    if detect_cycles(graph).has_cycle {
        // A normal-edge cycle already exists and is reported elsewhere.
        return None;
    }
    let mut combined = graph.clone();
    for edge in &mut combined.edges {
        if edge.r#type == EdgeType::Error {
            edge.r#type = EdgeType::Default;
        }
    }
    let result = detect_cycles(&combined);
    result.has_cycle.then_some(result.cycle_nodes)
}

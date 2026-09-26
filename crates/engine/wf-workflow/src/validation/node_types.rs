//! Per-node-type structural rules: boundary topology, isolated nodes, SYNC
//! path references, EMBED_GRAPH and SUBGRAPH declarations.

use std::collections::HashSet;

use wf_types::workflow_execution::WorkflowGraphStructure;
use wf_types::ValidationError;

use super::helpers::{fork_path_ids, incoming_edges, node_type_of, outgoing_edges};

/// Topological constraints of boundary nodes: START cannot have incoming
/// edges, END cannot have outgoing edges, and the triggered-subgraph
/// boundaries follow the same rule.
pub(super) fn validate_start_end_topology(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if let Some(ref start_id) = graph.start_node_id {
        if node_type_of(graph, start_id).is_some() && !incoming_edges(graph, start_id).is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", start_id),
                "START node cannot have incoming edges",
            ));
        }
    }

    for end_id in &graph.end_node_ids {
        if node_type_of(graph, end_id).is_some() && !outgoing_edges(graph, end_id).is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", end_id),
                format!("END node ({}) cannot have outgoing edges", end_id),
            ));
        }
    }

    for node in &graph.nodes {
        match node.node_type.as_str() {
            "START_FROM_MESSAGE" if !incoming_edges(graph, &node.id).is_empty() => {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    "START_FROM_MESSAGE node cannot have incoming edges",
                ));
            }
            "CONTINUE_FROM_MESSAGE" if !outgoing_edges(graph, &node.id).is_empty() => {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node.id),
                    "CONTINUE_FROM_MESSAGE node cannot have outgoing edges",
                ));
            }
            _ => {}
        }
    }

    errors
}

/// Boundary nodes are excluded; any other node without both an incoming
/// and an outgoing edge is reported as isolated. ERROR edges count as
/// connectivity, so a dedicated error handler entered only through an
/// error jump is not isolated.
pub(super) fn validate_isolated_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for node in &graph.nodes {
        if matches!(
            node.node_type.as_str(),
            "START" | "END" | "START_FROM_MESSAGE" | "CONTINUE_FROM_MESSAGE"
        ) {
            continue;
        }
        if incoming_edges(graph, &node.id).is_empty() && outgoing_edges(graph, &node.id).is_empty()
        {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!(
                    "Node ({}) is isolated, has no incoming or outgoing edges",
                    node.id
                ),
            ));
        }
    }

    errors
}

/// SYNC nodes must reference an existing fork path on both sides and
/// have well-formed variable mappings.
pub(super) fn validate_sync_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let mut fork_path_ids_set: HashSet<String> = HashSet::new();
    for node in &graph.nodes {
        if node.node_type == "FORK" {
            fork_path_ids_set.extend(fork_path_ids(node));
        }
    }

    for node in &graph.nodes {
        if node.node_type != "SYNC" {
            continue;
        }

        let source_path_id = node
            .inner
            .get("source_path_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .filter(|s| !s.is_empty());
        let target_path_id = node
            .inner
            .get("target_path_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .filter(|s| !s.is_empty());

        match &source_path_id {
            Some(path_id) if fork_path_ids_set.contains(path_id) => {}
            Some(path_id) => errors.push(ValidationError::new(
                format!("nodes.{}.config.source_path_id", node.id),
                format!(
                    "SYNC node '{}' has source_path_id '{}' that does not exist in any FORK node's fork_paths",
                    node.id, path_id
                ),
            )),
            None => errors.push(ValidationError::new(
                format!("nodes.{}.config.source_path_id", node.id),
                format!("SYNC node '{}' is missing required source_path_id", node.id),
            )),
        }

        if let Some(path_id) = &target_path_id {
            if !fork_path_ids_set.contains(path_id) {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.target_path_id", node.id),
                    format!(
                        "SYNC node '{}' has target_path_id '{}' that does not exist in any FORK node's fork_paths",
                        node.id, path_id
                    ),
                ));
            }
        }

        if let Some(mappings) = node
            .inner
            .get("variable_mappings")
            .and_then(|v| v.as_array())
        {
            for (idx, mapping) in mappings.iter().enumerate() {
                let has_source = mapping
                    .get("source_path")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty());
                let has_internal = mapping
                    .get("internal_name")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty());
                if !has_source {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.variable_mappings[{}]", node.id, idx),
                        format!(
                            "SYNC node '{}' has variableMapping with missing source_path",
                            node.id
                        ),
                    ));
                }
                if !has_internal {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.variable_mappings[{}]", node.id, idx),
                        format!(
                            "SYNC node '{}' has variableMapping with missing internal_name",
                            node.id
                        ),
                    ));
                }
            }
        }

        if incoming_edges(graph, &node.id).is_empty() && outgoing_edges(graph, &node.id).is_empty()
        {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!(
                    "SYNC node '{}' is isolated, has no incoming or outgoing edges",
                    node.id
                ),
            ));
        }
    }

    errors
}

/// EMBED_GRAPH nodes must reference an embed id or carry an inline graph
/// definition, and cannot declare variable mappings.
pub(super) fn validate_embed_graph(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for node in &graph.nodes {
        if node.node_type != "EMBED_GRAPH" {
            continue;
        }

        let has_embed_id = node
            .inner
            .get("embed_id")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        let has_inline = node
            .inner
            .get("graph_definition")
            .is_some_and(|v| !v.is_null());
        if !has_embed_id && !has_inline {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!(
                    "EMBED_GRAPH node ({}) is missing embed_id configuration",
                    node.id
                ),
            ));
        }

        let has_variable_inputs = node
            .inner
            .get("variable_inputs")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| !arr.is_empty());
        let has_variable_outputs = node
            .inner
            .get("variable_outputs")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| !arr.is_empty());
        if has_variable_inputs {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!(
                    "EMBED_GRAPH node '{}' should not have variable_inputs. Use SUBGRAPH for variable passing.",
                    node.id
                ),
            ));
        }
        if has_variable_outputs {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!(
                    "EMBED_GRAPH node '{}' should not have variable_outputs. Use SUBGRAPH for variable passing.",
                    node.id
                ),
            ));
        }
    }

    errors
}

/// SUBGRAPH nodes must reference a subgraph id (or embed id); the
/// variable mapping format is checked by the node-level validators.
pub(super) fn validate_subgraph_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for node in &graph.nodes {
        if node.node_type != "SUBGRAPH" {
            continue;
        }
        let has_id = node
            .inner
            .get("subgraph_id")
            .or_else(|| node.inner.get("embed_id"))
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if !has_id {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!(
                    "SUBGRAPH node ({}) is missing subgraph_id configuration",
                    node.id
                ),
            ));
        }
    }

    errors
}

//! Basic well-formedness rules: node ids/types, edge ids, edge endpoint
//! references, and boundary node counts (START/END vs triggered subgraph).

use std::collections::{HashMap, HashSet};

use wf_types::workflow_execution::WorkflowGraphStructure;
use wf_types::ValidationError;

use super::helpers::node_type_map;

pub(super) fn validate_nodes(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut node_ids = HashSet::new();

    if graph.nodes.is_empty() {
        errors.push(ValidationError::new(
            "nodes",
            "Graph must have at least one node",
        ));
        return errors;
    }

    for node in &graph.nodes {
        if node.id.is_empty() {
            errors.push(ValidationError::new("nodes", "Node ID cannot be empty"));
        }
        if node.node_type.is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!("Node '{}' has no type", node.id),
            ));
        }
        if !node_ids.insert(node.id.clone()) {
            errors.push(ValidationError::new(
                format!("nodes.{}", node.id),
                format!("Duplicate node ID: {}", node.id),
            ));
        }
    }

    errors
}

pub(super) fn validate_edges(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut edge_ids = HashSet::new();

    for edge in &graph.edges {
        if edge.id.is_empty() {
            errors.push(ValidationError::new("edges", "Edge ID cannot be empty"));
        }
        if !edge_ids.insert(edge.id.clone()) {
            errors.push(ValidationError::new(
                format!("edges.{}", edge.id),
                format!("Duplicate edge ID: {}", edge.id),
            ));
        }
    }

    errors
}

pub(super) fn validate_start_end(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let node_types: HashMap<&str, &str> = node_type_map(graph);

    let start_count = node_types.values().filter(|t| **t == "START").count();
    let end_count = node_types.values().filter(|t| **t == "END").count();
    let message_start_count = node_types
        .values()
        .filter(|t| **t == "START_FROM_MESSAGE")
        .count();
    let message_end_count = node_types
        .values()
        .filter(|t| **t == "CONTINUE_FROM_MESSAGE")
        .count();

    let has_special = message_start_count > 0 || message_end_count > 0;

    if has_special {
        if message_start_count != 1 {
            errors.push(ValidationError::new(
                "nodes",
                "Triggered subgraph must have exactly one START_FROM_MESSAGE node",
            ));
        }
        if message_end_count != 1 {
            errors.push(ValidationError::new(
                "nodes",
                "Triggered subgraph must have exactly one CONTINUE_FROM_MESSAGE node",
            ));
        }
        if start_count > 0 {
            errors.push(ValidationError::new(
                "nodes",
                "Triggered subgraph cannot contain START node",
            ));
        }
        if end_count > 0 {
            errors.push(ValidationError::new(
                "nodes",
                "Triggered subgraph cannot contain END node",
            ));
        }
    } else {
        if start_count == 0 {
            errors.push(ValidationError::new(
                "nodes",
                "Workflow must have a START node",
            ));
        } else if start_count > 1 {
            errors.push(ValidationError::new(
                "nodes",
                "Workflow must have exactly one START node",
            ));
        }

        if end_count == 0 {
            errors.push(ValidationError::new(
                "nodes",
                "Workflow must have at least one END node",
            ));
        }
    }

    if let Some(ref start_id) = graph.start_node_id {
        if !node_types.contains_key(start_id.as_str()) {
            errors.push(ValidationError::new(
                "start_node_id",
                format!("Start node '{}' not found in nodes", start_id),
            ));
        }
    } else if !has_special {
        errors.push(ValidationError::new(
            "start_node_id",
            "Graph must have a start_node_id",
        ));
    }

    errors
}

pub(super) fn validate_references(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();

    for edge in &graph.edges {
        if !node_ids.contains(edge.source_node_id.as_str()) {
            errors.push(ValidationError::new(
                format!("edges.{}", edge.id),
                format!("Edge source '{}' not found in nodes", edge.source_node_id),
            ));
        }
        if !node_ids.contains(edge.target_node_id.as_str()) {
            errors.push(ValidationError::new(
                format!("edges.{}", edge.id),
                format!("Edge target '{}' not found in nodes", edge.target_node_id),
            ));
        }
    }

    errors
}

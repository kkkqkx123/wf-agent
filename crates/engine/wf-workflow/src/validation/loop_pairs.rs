//! LOOP_START/LOOP_END pairing rules: loop id uniqueness, cross references,
//! and required boundary edges.

use std::collections::{HashMap, HashSet};

use wf_types::workflow_execution::WorkflowGraphStructure;
use wf_types::ValidationError;

use super::helpers::{incoming_edges, outgoing_edges};

/// LOOP_START/LOOP_END pairing: each loopId must have exactly one of
/// each, cross references must resolve, and boundary nodes must have
/// edges on the required sides.
pub(super) fn validate_loop_pairs(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let mut loop_starts: HashMap<String, String> = HashMap::new(); // node_id -> loop_id
    let mut loop_ends: HashMap<String, (String, Option<String>)> = HashMap::new(); // node_id -> (loop_id, loop_start_node_id)

    for node in &graph.nodes {
        match node.node_type.as_str() {
            "LOOP_START" => {
                let loop_id = node
                    .inner
                    .get("loop_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .filter(|s| !s.is_empty());
                match loop_id {
                    Some(id) => {
                        loop_starts.insert(node.id.clone(), id);
                    }
                    None => errors.push(ValidationError::new(
                        format!("nodes.{}", node.id),
                        format!(
                            "LOOP_START node ({}) must have a non-empty loop_id in its config",
                            node.id
                        ),
                    )),
                }
            }
            "LOOP_END" => {
                let loop_id = node
                    .inner
                    .get("loop_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .filter(|s| !s.is_empty());
                let loop_start_node_id = node
                    .inner
                    .get("loop_start_node_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .filter(|s| !s.is_empty());
                match loop_id {
                    Some(id) => {
                        loop_ends.insert(node.id.clone(), (id, loop_start_node_id));
                    }
                    None => errors.push(ValidationError::new(
                        format!("nodes.{}", node.id),
                        format!(
                            "LOOP_END node ({}) must have a non-empty loop_id in its config",
                            node.id
                        ),
                    )),
                }
            }
            _ => {}
        }
    }

    // Duplicate loop ids per node kind.
    let start_ids_by_loop: HashMap<String, Vec<String>> =
        loop_starts
            .iter()
            .fold(HashMap::new(), |mut acc, (nid, lid)| {
                acc.entry(lid.clone()).or_default().push(nid.clone());
                acc
            });
    let end_ids_by_loop: HashMap<String, Vec<String>> =
        loop_ends
            .iter()
            .fold(HashMap::new(), |mut acc, (nid, (lid, _))| {
                acc.entry(lid.clone()).or_default().push(nid.clone());
                acc
            });
    for (loop_id, node_ids) in &start_ids_by_loop {
        if node_ids.len() > 1 {
            errors.push(ValidationError::new(
                "nodes",
                format!(
                    "Multiple LOOP_START nodes share the same loopId ({}): [{}]",
                    loop_id,
                    node_ids.join(", ")
                ),
            ));
        }
    }
    for (loop_id, node_ids) in &end_ids_by_loop {
        if node_ids.len() > 1 {
            errors.push(ValidationError::new(
                "nodes",
                format!(
                    "Multiple LOOP_END nodes share the same loopId ({}): [{}]",
                    loop_id,
                    node_ids.join(", ")
                ),
            ));
        }
    }

    // Pairing and boundary edges.
    for (node_id, loop_id) in &loop_starts {
        let ends = end_ids_by_loop
            .get(loop_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if ends.is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", node_id),
                format!(
                    "LOOP_START node ({}) with loopId ({}) has no matching LOOP_END node",
                    node_id, loop_id
                ),
            ));
        }
        if outgoing_edges(graph, node_id).is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", node_id),
                format!(
                    "LOOP_START node ({}) must have at least one outgoing edge",
                    node_id
                ),
            ));
        }
        if incoming_edges(graph, node_id).is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", node_id),
                format!(
                    "LOOP_START node ({}) must have at least one incoming edge",
                    node_id
                ),
            ));
        }
    }

    let loop_start_ids: HashSet<&str> = loop_starts.keys().map(String::as_str).collect();
    for (node_id, (loop_id, loop_start_node_id)) in &loop_ends {
        let starts = start_ids_by_loop
            .get(loop_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if starts.is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", node_id),
                format!(
                    "LOOP_END node ({}) with loopId ({}) has no matching LOOP_START node",
                    node_id, loop_id
                ),
            ));
        }
        if outgoing_edges(graph, node_id).is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", node_id),
                format!(
                    "LOOP_END node ({}) must have at least one outgoing edge",
                    node_id
                ),
            ));
        }

        if let Some(referenced) = loop_start_node_id {
            if !loop_start_ids.contains(referenced.as_str()) {
                errors.push(ValidationError::new(
                    format!("nodes.{}", node_id),
                    format!(
                        "LOOP_END node ({}) references non-existent LOOP_START node ({}) via loop_start_node_id",
                        node_id, referenced
                    ),
                ));
            } else if let Some(start_loop_id) = loop_starts.get(referenced) {
                if start_loop_id != loop_id {
                    errors.push(ValidationError::new(
                        format!("nodes.{}", node_id),
                        format!(
                            "LOOP_END node ({}) loopId ({}) does not match the loopId ({}) of the referenced LOOP_START node ({})",
                            node_id, loop_id, start_loop_id, referenced
                        ),
                    ));
                }
            }
        }
    }

    errors
}

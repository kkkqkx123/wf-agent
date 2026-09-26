//! FORK/JOIN pairing rules: path id uniqueness, branch/join declarations,
//! pairing by the first fork path id, and FORK→JOIN reachability.

use std::collections::{HashMap, HashSet};

use wf_types::workflow_execution::{WorkflowGraphStructure, WorkflowNode};
use wf_types::ValidationError;

use super::helpers::{fork_path_ids, incoming_edges, join_path_ids, outgoing_edges};
use crate::analysis::get_reachable_nodes;

/// FORK/JOIN pairing: every FORK must have branches and a matching JOIN,
/// every JOIN must match a FORK; paired path ids must agree as sets with
/// matching counts; empty or duplicate path ids fail; the JOIN must be
/// reachable from its FORK. Threshold compatibility for `wait_for_n`
/// lives in the node-level JOIN validator where the path count is known.
pub(super) fn validate_fork_join_pairs(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let fork_nodes: Vec<&WorkflowNode> = graph
        .nodes
        .iter()
        .filter(|n| n.node_type == "FORK")
        .collect();
    let join_nodes: Vec<&WorkflowNode> = graph
        .nodes
        .iter()
        .filter(|n| n.node_type == "JOIN")
        .collect();

    // Empty path ids fail with a dedicated message before uniqueness.
    for fork in &fork_nodes {
        for (idx, raw) in fork
            .inner
            .get("fork_paths")
            .and_then(|v| v.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            let empty = raw
                .get("path_id")
                .and_then(|v| v.as_str())
                .is_none_or(|s| s.trim().is_empty());
            if empty {
                errors.push(ValidationError::new(
                    format!("nodes.{}.config.fork_paths[{}].path_id", fork.id, idx),
                    format!(
                        "FORK node '{}' has an empty path_id; each branch needs a non-empty unique path id",
                        fork.id
                    ),
                ));
            }
        }
    }
    for join in &join_nodes {
        if let Some(arr) = join.inner.get("fork_path_ids").and_then(|v| v.as_array()) {
            for (idx, entry) in arr.iter().enumerate() {
                if entry.as_str().is_none_or(|s| s.trim().is_empty()) {
                    errors.push(ValidationError::new(
                        format!("nodes.{}.config.fork_path_ids[{}]", join.id, idx),
                        format!(
                            "JOIN node '{}' has an empty path id; each entry must be a non-empty string",
                            join.id
                        ),
                    ));
                }
            }
        }
    }

    // Global path id uniqueness across all FORK nodes.
    let mut all_path_ids: HashSet<String> = HashSet::new();
    for fork in &fork_nodes {
        for path_id in fork_path_ids(fork) {
            if path_id.trim().is_empty() {
                continue;
            }
            if !all_path_ids.insert(path_id.clone()) {
                errors.push(ValidationError::new(
                    format!("nodes.{}", fork.id),
                    format!(
                        "pathId ({}) of FORK node ({}) is not unique within the workflow definition",
                        path_id, fork.id
                    ),
                ));
            }
        }
    }

    // FORK nodes must declare branches and have outgoing edges.
    for fork in &fork_nodes {
        let outgoing = outgoing_edges(graph, &fork.id);
        if outgoing.is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", fork.id),
                format!("FORK node '{}' has no outgoing edges", fork.id),
            ));
        }

        let branches = fork.inner.get("fork_paths").and_then(|b| b.as_array());
        match branches {
            None => errors.push(ValidationError::new(
                format!("nodes.{}", fork.id),
                format!(
                    "FORK node '{}' must define a non-empty fork_paths array",
                    fork.id
                ),
            )),
            Some(branches) if branches.is_empty() => errors.push(ValidationError::new(
                format!("nodes.{}", fork.id),
                format!("FORK node '{}' has empty fork_paths", fork.id),
            )),
            _ => {}
        }
    }

    // JOIN nodes must have incoming edges.
    for join in &join_nodes {
        if incoming_edges(graph, &join.id).is_empty() {
            errors.push(ValidationError::new(
                format!("nodes.{}", join.id),
                format!("JOIN node '{}' has no incoming edges", join.id),
            ));
        }
    }

    // Pairing by the first fork path id.
    let join_by_first_path: HashMap<String, &WorkflowNode> = join_nodes
        .iter()
        .filter_map(|j| join_path_ids(j).into_iter().next().map(|first| (first, *j)))
        .collect();

    let mut pairs: Vec<(&WorkflowNode, &WorkflowNode)> = Vec::new();
    let mut paired_joins: HashSet<&str> = HashSet::new();

    for fork in &fork_nodes {
        let path_ids = fork_path_ids(fork);
        let matched: Option<&WorkflowNode> = path_ids
            .first()
            .and_then(|first| join_by_first_path.get(first).copied());

        match matched {
            Some(join) => {
                let join_ids = join_path_ids(join);
                let fork_ids = fork_path_ids(fork);
                if !fork_ids.is_empty() && !join_ids.is_empty() {
                    let mut sorted_fork = fork_ids.clone();
                    let mut sorted_join = join_ids.clone();
                    sorted_fork.sort();
                    sorted_join.sort();
                    if sorted_fork != sorted_join {
                        errors.push(ValidationError::new(
                            format!("nodes.{}", fork.id),
                            format!(
                                "fork_path_ids of FORK node ({}) and JOIN node ({}) do not match: FORK has {} path(s) [{}], JOIN has {} path(s) [{}]",
                                fork.id,
                                join.id,
                                fork_ids.len(),
                                fork_ids.join(", "),
                                join_ids.len(),
                                join_ids.join(", "),
                            ),
                        ));
                    } else {
                        pairs.push((fork, join));
                        paired_joins.insert(join.id.as_str());
                    }
                } else {
                    pairs.push((fork, join));
                    paired_joins.insert(join.id.as_str());
                }
            }
            None => {
                errors.push(ValidationError::new(
                    format!("nodes.{}", fork.id),
                    format!("FORK node ({}) has no matching JOIN node", fork.id),
                ));
            }
        }
    }

    for join in &join_nodes {
        if !paired_joins.contains(join.id.as_str()) {
            errors.push(ValidationError::new(
                format!("nodes.{}", join.id),
                format!("JOIN node ({}) has no matching FORK node", join.id),
            ));
        }
    }

    // Reachability from FORK to its paired JOIN.
    for (fork, join) in &pairs {
        let reachable = get_reachable_nodes(graph, &fork.id);
        if !reachable.contains(&join.id) {
            errors.push(ValidationError::new(
                format!("nodes.{}", fork.id),
                format!(
                    "FORK node ({}) cannot reach the paired JOIN node ({})",
                    fork.id, join.id
                ),
            ));
        }
    }

    errors
}

/// FORK branch `child_node_id` entries must resolve to real graph nodes.
pub(super) fn validate_fork_children(graph: &WorkflowGraphStructure) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    for node in &graph.nodes {
        if node.node_type != "FORK" {
            continue;
        }
        if let Some(paths) = node.inner.get("fork_paths").and_then(|v| v.as_array()) {
            for (idx, path) in paths.iter().enumerate() {
                if let Some(child) = path
                    .get("child_node_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    if !node_ids.contains(child) {
                        errors.push(ValidationError::new(
                            format!("nodes.{}.config.fork_paths[{}]", node.id, idx),
                            format!(
                                "FORK node '{}' branch targets unknown node '{}'",
                                node.id, child
                            ),
                        ));
                    }
                }
            }
        }
    }
    errors
}

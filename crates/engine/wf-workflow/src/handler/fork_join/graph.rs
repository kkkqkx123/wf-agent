use std::collections::{HashMap, HashSet};

use wf_execution_shared::context::NodeExecutionContext;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

/// Extract the subgraph reachable from a fork branch edge, excluding the
/// join node. Uses BFS so nested forks, parallel sub-branches and converging
/// paths inside the branch are all collected; the join node belongs to the
/// parent graph only (branches end at the node before the join).
pub fn extract_branch_subgraph(
    graph: &WorkflowGraphStructure,
    _fork_node_id: &str,
    branch_edge: &WorkflowEdge,
    join_node_id: &str,
) -> WorkflowGraphStructure {
    let mut branch_nodes: HashSet<String> = HashSet::new();
    let mut branch_edges: Vec<WorkflowEdge> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();

    let edge_map: HashMap<&str, Vec<&WorkflowEdge>> = graph
        .edges
        .iter()
        // ERROR edges never define fork branch structure: an error jump must
        // not expand a branch subgraph or its boundaries.
        .filter(|e| !crate::error_branch::is_error_edge(e))
        .fold(HashMap::new(), |mut acc, e| {
            acc.entry(e.source_node_id.as_str())
                .or_insert_with(Vec::new)
                .push(e);
            acc
        });

    branch_nodes.insert(branch_edge.target_node_id.clone());
    queue.push_back(branch_edge.target_node_id.clone());

    while let Some(current) = queue.pop_front() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if let Some(edges) = edge_map.get(current.as_str()) {
            for edge in edges {
                // The branch ends at the node before the join; the join is
                // never part of a branch subgraph.
                if edge.target_node_id == join_node_id {
                    continue;
                }
                branch_edges.push((*edge).clone());
                branch_nodes.insert(edge.target_node_id.clone());
                if !visited.contains(&edge.target_node_id) {
                    queue.push_back(edge.target_node_id.clone());
                }
            }
        }
    }

    let nodes: Vec<WorkflowNode> = graph
        .nodes
        .iter()
        .filter(|n| branch_nodes.contains(&n.id))
        .cloned()
        .collect();

    // End nodes of the branch subgraph: nodes with no outgoing edges within
    // the branch (their only outgoing edges point at the join, which belongs
    // to the parent graph).
    let end_node_ids: Vec<String> = nodes
        .iter()
        .map(|n| n.id.clone())
        .filter(|nid| branch_edges.iter().all(|e| &e.source_node_id != nid))
        .collect();

    WorkflowGraphStructure {
        nodes,
        edges: branch_edges,
        start_node_id: Some(branch_edge.target_node_id.clone()),
        end_node_ids,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        error_default: None,
    }
}

/// Find the JOIN node that the fork's branches converge to, structurally:
/// follow the graph edges from every branch edge of the fork and pick the
/// earliest JOIN-type node reachable from *all* branches. Nested forks stay
/// out of the result because their join is reachable from only one of the
/// outer fork's branches (the intersection of the per-branch reachable sets
/// is empty for them). Replaces the former `fork_path_ids` string matching
/// between FORK and JOIN configs, which broke across forks sharing path ids.
pub fn find_join_node(graph: &WorkflowGraphStructure, fork_node_id: &str) -> Option<String> {
    let branch_edges: Vec<&WorkflowEdge> = graph
        .edges
        .iter()
        .filter(|e| e.source_node_id == fork_node_id && !crate::error_branch::is_error_edge(e))
        .collect();
    if branch_edges.is_empty() {
        return None;
    }

    // Per-branch BFS distance maps (visited set guards cycles).
    let mut distances: Vec<HashMap<String, u32>> = Vec::new();
    for edge in &branch_edges {
        let mut dist: HashMap<String, u32> = HashMap::new();
        let mut queue = std::collections::VecDeque::new();
        dist.insert(edge.target_node_id.clone(), 0);
        queue.push_back(edge.target_node_id.clone());
        while let Some(current) = queue.pop_front() {
            let depth = dist[&current];
            for next in graph
                .edges
                .iter()
                .filter(|e| e.source_node_id == current && !crate::error_branch::is_error_edge(e))
                .map(|e| &e.target_node_id)
            {
                if !dist.contains_key(next) {
                    dist.insert(next.clone(), depth + 1);
                    queue.push_back(next.clone());
                }
            }
        }
        distances.push(dist);
    }

    // The join must be reachable from every branch: intersect the sets.
    let mut common: HashSet<String> = distances[0].keys().cloned().collect();
    for dist in &distances[1..] {
        common.retain(|id| dist.contains_key(id));
    }
    if common.is_empty() {
        return None;
    }

    // Earliest common JOIN: minimize the maximum branch distance.
    common
        .into_iter()
        .filter(|id| {
            graph
                .nodes
                .iter()
                .any(|n| &n.id == id && n.node_type == "JOIN")
        })
        .min_by_key(|id| {
            distances
                .iter()
                .map(|dist| dist[id])
                .max()
                .unwrap_or(u32::MAX)
        })
}

/// Locate the FORK node whose `fork_paths` contain `path_id`. Used by SYNC
/// nodes to find the fork that launched their source branch.
pub fn find_fork_by_path(graph: &WorkflowGraphStructure, path_id: &str) -> Option<String> {
    graph
        .nodes
        .iter()
        .find(|n| {
            n.node_type == "FORK"
                && n.inner
                    .get("fork_paths")
                    .and_then(|p| p.as_array())
                    .is_some_and(|paths| {
                        paths
                            .iter()
                            .any(|p| p.get("path_id").and_then(|v| v.as_str()) == Some(path_id))
                    })
        })
        .map(|n| n.id.clone())
}

/// Locate the FORK node whose `fork_paths` match this JOIN's `fork_path_ids`.
pub fn find_fork_node(
    ctx: &NodeExecutionContext,
    graph: &WorkflowGraphStructure,
) -> Option<String> {
    let path_ids: Vec<&str> = ctx
        .node_config
        .as_ref()
        .and_then(|c| c.get("fork_path_ids"))
        .and_then(|v| v.as_array())
        .map(|ids| ids.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    graph
        .nodes
        .iter()
        .find(|n| {
            n.node_type == "FORK"
                && n.inner
                    .get("fork_paths")
                    .and_then(|p| p.as_array())
                    .map(|paths| {
                        paths
                            .iter()
                            .filter_map(|p| p.get("path_id").and_then(|v| v.as_str()))
                            .collect::<Vec<_>>()
                            == path_ids
                    })
                    .unwrap_or(false)
        })
        .map(|n| n.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn bfs_extracts_nested_fork_branch() {
        let nodes = vec![
            WorkflowNode {
                id: "fork".into(),
                name: None,
                node_type: "FORK".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "a1".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "a2".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "nfork".into(),
                name: None,
                node_type: "FORK".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "b1".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "b2".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "njoin".into(),
                name: None,
                node_type: "JOIN".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "join".into(),
                name: None,
                node_type: "JOIN".into(),
                inner: Value::Null,
            },
        ];
        let edges = vec![
            edge("fork", "a1"),
            edge("fork", "a2"),
            edge("a1", "nfork"),
            edge("a2", "njoin"),
            edge("nfork", "b1"),
            edge("nfork", "b2"),
            edge("b1", "njoin"),
            edge("b2", "njoin"),
            edge("njoin", "join"),
            edge("a2", "join"),
        ];
        let graph = build_graph(nodes, edges);

        let subgraph = extract_branch_subgraph(
            &graph,
            "fork",
            graph
                .edges
                .iter()
                .find(|e| e.source_node_id == "fork" && e.target_node_id == "a1")
                .unwrap(),
            "join",
        );
        let mut ids: Vec<String> = subgraph.nodes.iter().map(|n| n.id.clone()).collect();
        ids.sort();
        assert_eq!(ids, vec!["a1", "b1", "b2", "nfork", "njoin"]);
        assert!(subgraph.end_node_ids.contains(&"njoin".to_string()));

        let subgraph2 = extract_branch_subgraph(
            &graph,
            "fork",
            graph
                .edges
                .iter()
                .find(|e| e.source_node_id == "fork" && e.target_node_id == "a2")
                .unwrap(),
            "join",
        );
        let mut ids2: Vec<String> = subgraph2.nodes.iter().map(|n| n.id.clone()).collect();
        ids2.sort();
        assert_eq!(ids2, vec!["a2", "njoin"]);
        assert!(subgraph2.end_node_ids.contains(&"njoin".to_string()));
    }

    #[test]
    fn find_join_node_finds_earliest_common_join() {
        let nodes = vec![
            WorkflowNode {
                id: "fork".into(),
                name: None,
                node_type: "FORK".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "a1".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "b1".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "join".into(),
                name: None,
                node_type: "JOIN".into(),
                inner: Value::Null,
            },
        ];
        let graph = build_graph(
            nodes,
            vec![
                edge("fork", "a1"),
                edge("fork", "b1"),
                edge("a1", "join"),
                edge("b1", "join"),
            ],
        );
        assert_eq!(find_join_node(&graph, "fork"), Some("join".to_string()));

        let dangling = build_graph(
            vec![
                WorkflowNode {
                    id: "fork".into(),
                    name: None,
                    node_type: "FORK".into(),
                    inner: Value::Null,
                },
                WorkflowNode {
                    id: "a1".into(),
                    name: None,
                    node_type: "VARIABLE".into(),
                    inner: Value::Null,
                },
                WorkflowNode {
                    id: "b1".into(),
                    name: None,
                    node_type: "VARIABLE".into(),
                    inner: Value::Null,
                },
            ],
            vec![edge("fork", "a1"), edge("fork", "b1")],
        );
        assert_eq!(find_join_node(&dangling, "fork"), None);

        let no_branches = build_graph(
            vec![WorkflowNode {
                id: "fork".into(),
                name: None,
                node_type: "FORK".into(),
                inner: Value::Null,
            }],
            vec![],
        );
        assert_eq!(find_join_node(&no_branches, "fork"), None);
    }

    #[test]
    fn find_join_node_ignores_nested_fork_join() {
        let nodes = vec![
            WorkflowNode {
                id: "ofork".into(),
                name: None,
                node_type: "FORK".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "oa".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "nfork".into(),
                name: None,
                node_type: "FORK".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "ia".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "ijoin".into(),
                name: None,
                node_type: "JOIN".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "ob".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "ob2".into(),
                name: None,
                node_type: "VARIABLE".into(),
                inner: Value::Null,
            },
            WorkflowNode {
                id: "ojoin".into(),
                name: None,
                node_type: "JOIN".into(),
                inner: Value::Null,
            },
        ];
        let graph = build_graph(
            nodes,
            vec![
                edge("ofork", "oa"),
                edge("ofork", "ob"),
                edge("oa", "nfork"),
                edge("nfork", "ia"),
                edge("ia", "ijoin"),
                edge("ijoin", "ojoin"),
                edge("ob", "ob2"),
                edge("ob2", "ojoin"),
            ],
        );
        assert_eq!(find_join_node(&graph, "ofork"), Some("ojoin".to_string()));
    }

    fn edge(source: &str, target: &str) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{}-{}", source, target),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: wf_types::workflow::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            error_route: None,
        }
    }

    fn build_graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
            error_default: None,
        }
    }
}

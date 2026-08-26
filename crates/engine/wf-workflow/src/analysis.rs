use std::collections::{HashMap, HashSet, VecDeque};

use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure};

/// An edge from a LOOP_END node back to its LOOP_START node represents the
/// loop continuation, not a structural cycle. Such edges are excluded from
/// cycle detection and topological sort.
fn is_loop_back_edge(graph: &WorkflowGraphStructure, edge: &WorkflowEdge) -> bool {
    let node_type_of = |id: &str| -> Option<&str> {
        graph
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.node_type.as_str())
    };
    node_type_of(&edge.source_node_id) == Some("LOOP_END")
        && node_type_of(&edge.target_node_id) == Some("LOOP_START")
}

fn find_edge<'a>(
    graph: &'a WorkflowGraphStructure,
    source: &str,
    target: &str,
) -> Option<&'a WorkflowEdge> {
    graph
        .edges
        .iter()
        .find(|e| e.source_node_id == source && e.target_node_id == target)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CycleDetectionResult {
    pub has_cycle: bool,
    pub cycle_nodes: Vec<String>,
    pub cycle_edges: Vec<String>,
}

/// Detect structural cycles in a workflow graph using iterative DFS with a
/// recursion stack. Loop continuation edges (LOOP_END -> LOOP_START) are
/// treated as legal control flow, not cycles.
pub fn detect_cycles(graph: &WorkflowGraphStructure) -> CycleDetectionResult {
    let outgoing: HashMap<&str, Vec<&WorkflowEdge>> =
        graph.edges.iter().fold(HashMap::new(), |mut acc, e| {
            acc.entry(e.source_node_id.as_str())
                .or_insert_with(Vec::new)
                .push(e);
            acc
        });

    // 0 = unvisited, 1 = on recursion stack, 2 = fully explored.
    let mut state: HashMap<&str, u8> = HashMap::new();

    for start in &graph.nodes {
        if state.get(start.id.as_str()) == Some(&2) || state.get(start.id.as_str()) == Some(&1) {
            continue;
        }

        let mut stack: Vec<(&str, usize)> = vec![(start.id.as_str(), 0)];
        let mut path: Vec<&str> = vec![start.id.as_str()];
        state.insert(start.id.as_str(), 1);

        while let Some((current, idx)) = stack.last_mut() {
            let current = *current;
            let edges = outgoing.get(current);

            if let Some(edges) = edges {
                if *idx >= edges.len() {
                    state.insert(current, 2);
                    path.pop();
                    stack.pop();
                    continue;
                }
                let edge = edges[*idx];
                *idx += 1;
                let next = edge.target_node_id.as_str();
                match state.get(next).copied().unwrap_or(0) {
                    0 => {
                        state.insert(next, 1);
                        path.push(next);
                        stack.push((next, 0));
                    }
                    1 => {
                        if is_loop_back_edge(graph, edge) {
                            continue;
                        }
                        let cycle_start = path.iter().position(|p| *p == next).unwrap_or(0);
                        let nodes: Vec<&str> = path[cycle_start..].to_vec();
                        let mut cycle_edges: Vec<String> = Vec::new();
                        for pair in nodes.windows(2) {
                            if let Some(e) = find_edge(graph, pair[0], pair[1]) {
                                cycle_edges.push(e.id.clone());
                            }
                        }
                        cycle_edges.push(edge.id.clone());
                        return CycleDetectionResult {
                            has_cycle: true,
                            cycle_nodes: nodes.into_iter().map(String::from).collect(),
                            cycle_edges,
                        };
                    }
                    _ => {}
                }
            } else {
                state.insert(current, 2);
                path.pop();
                stack.pop();
            }
        }
    }

    CycleDetectionResult::default()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TopologicalSortResult {
    pub success: bool,
    pub sorted_nodes: Vec<String>,
    pub cycle_nodes: Vec<String>,
}

/// Topological sort of the workflow graph using Kahn's algorithm. Loop
/// continuation edges are excluded so LOOP constructs do not fail the sort.
pub fn topological_sort(graph: &WorkflowGraphStructure) -> TopologicalSortResult {
    let mut in_degree: HashMap<&str, usize> =
        graph.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

    for edge in &graph.edges {
        if is_loop_back_edge(graph, edge) {
            continue;
        }
        *in_degree
            .get_mut(edge.target_node_id.as_str())
            .unwrap_or(&mut 0) += 1;
        adj.entry(edge.source_node_id.as_str())
            .or_default()
            .push(edge.target_node_id.as_str());
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(node, _)| *node)
        .collect();

    let mut sorted: Vec<String> = Vec::new();
    while let Some(node) = queue.pop_front() {
        sorted.push(node.to_string());
        if let Some(neighbors) = adj.get(node) {
            for neighbor in neighbors {
                let degree = in_degree.get_mut(*neighbor).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(neighbor);
                }
            }
        }
    }

    let success = sorted.len() == graph.nodes.len();
    let cycle_nodes = if success {
        Vec::new()
    } else {
        detect_cycles(graph).cycle_nodes
    };
    TopologicalSortResult {
        success,
        sorted_nodes: sorted,
        cycle_nodes,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReachabilityResult {
    pub reachable_from_start: HashSet<String>,
    pub reachable_to_end: HashSet<String>,
    pub unreachable_nodes: Vec<String>,
    pub dead_end_nodes: Vec<String>,
}

/// All nodes reachable from `start_id` following outgoing edges.
pub fn get_reachable_nodes(graph: &WorkflowGraphStructure, start_id: &str) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start_id.to_string());
    visited.insert(start_id.to_string());

    while let Some(current) = queue.pop_front() {
        for edge in &graph.edges {
            if edge.source_node_id == current && visited.insert(edge.target_node_id.clone()) {
                queue.push_back(edge.target_node_id.clone());
            }
        }
    }
    visited
}

/// All nodes that can reach `target_id` following edges in reverse.
pub fn get_nodes_reaching_to(graph: &WorkflowGraphStructure, target_id: &str) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(target_id.to_string());
    visited.insert(target_id.to_string());

    while let Some(current) = queue.pop_front() {
        for edge in &graph.edges {
            if edge.target_node_id == current && visited.insert(edge.source_node_id.clone()) {
                queue.push_back(edge.source_node_id.clone());
            }
        }
    }
    visited
}

/// Reachability analysis: nodes reachable from START, nodes reaching END,
/// unreachable nodes and dead-end nodes (reachable from START but unable to
/// reach any END node).
pub fn analyze_reachability(graph: &WorkflowGraphStructure) -> ReachabilityResult {
    let all: HashSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();

    let reachable_from_start = graph
        .start_node_id
        .as_deref()
        .map(|start| get_reachable_nodes(graph, start))
        .unwrap_or_default();

    let mut reachable_to_end = HashSet::new();
    for end_id in &graph.end_node_ids {
        reachable_to_end.extend(get_nodes_reaching_to(graph, end_id));
    }

    let mut unreachable_nodes: Vec<String> = all
        .iter()
        .filter(|id| !reachable_from_start.contains(*id))
        .cloned()
        .collect();
    unreachable_nodes.sort();

    let mut dead_end_nodes: Vec<String> = all
        .iter()
        .filter(|id| reachable_from_start.contains(*id) && !reachable_to_end.contains(*id))
        .cloned()
        .collect();
    dead_end_nodes.sort();

    ReachabilityResult {
        reachable_from_start,
        reachable_to_end,
        unreachable_nodes,
        dead_end_nodes,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphAnalysis {
    pub cycle_detection: CycleDetectionResult,
    pub topological_sort: TopologicalSortResult,
    pub reachability: ReachabilityResult,
    pub node_total: usize,
    pub edge_total: usize,
    pub node_counts_by_type: HashMap<String, usize>,
}

/// Combined graph analysis used by validation and pre-execution checks.
pub fn analyze_graph(graph: &WorkflowGraphStructure) -> GraphAnalysis {
    let mut node_counts_by_type: HashMap<String, usize> = HashMap::new();
    for node in &graph.nodes {
        *node_counts_by_type
            .entry(node.node_type.clone())
            .or_insert(0) += 1;
    }

    GraphAnalysis {
        cycle_detection: detect_cycles(graph),
        topological_sort: topological_sort(graph),
        reachability: analyze_reachability(graph),
        node_total: graph.nodes.len(),
        edge_total: graph.edges.len(),
        node_counts_by_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    fn node(id: &str, node_type: &str) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner: serde_json::json!({}),
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

    fn graph(
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
            start_node_id: start.map(String::from),
            end_node_ids: ends.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn detects_simple_cycle() {
        let g = graph(
            vec![node("a", "VARIABLE"), node("b", "VARIABLE")],
            vec![edge("a", "b"), edge("b", "a")],
            Some("a"),
            vec!["b"],
        );
        let result = detect_cycles(&g);
        assert!(result.has_cycle);
        assert!(!result.cycle_nodes.is_empty());
    }

    #[test]
    fn acyclic_graph_has_no_cycle() {
        let g = graph(
            vec![node("start", "START"), node("end", "END")],
            vec![edge("start", "end")],
            Some("start"),
            vec!["end"],
        );
        assert!(!detect_cycles(&g).has_cycle);
    }

    #[test]
    fn loop_continuation_edge_is_not_a_cycle() {
        let g = graph(
            vec![
                node("start", "START"),
                node("ls", "LOOP_START"),
                node("body", "VARIABLE"),
                node("le", "LOOP_END"),
                node("end", "END"),
            ],
            vec![
                edge("start", "ls"),
                edge("ls", "body"),
                edge("body", "le"),
                edge("le", "ls"),
                edge("le", "end"),
            ],
            Some("start"),
            vec!["end"],
        );
        let result = detect_cycles(&g);
        assert!(!result.has_cycle, "loop back edge must not be a cycle");

        let topo = topological_sort(&g);
        assert!(topo.success, "loop graph must be topologically sortable");
        assert_eq!(topo.sorted_nodes.len(), 5);
    }

    #[test]
    fn topological_sort_orders_linear_graph() {
        let g = graph(
            vec![
                node("start", "START"),
                node("a", "VARIABLE"),
                node("end", "END"),
            ],
            vec![edge("start", "a"), edge("a", "end")],
            Some("start"),
            vec!["end"],
        );
        let result = topological_sort(&g);
        assert!(result.success);
        assert_eq!(
            result.sorted_nodes,
            vec!["start".to_string(), "a".to_string(), "end".to_string()]
        );
    }

    #[test]
    fn topological_sort_fails_on_cycle_with_nodes() {
        let g = graph(
            vec![node("a", "VARIABLE"), node("b", "VARIABLE")],
            vec![edge("a", "b"), edge("b", "a")],
            None,
            vec![],
        );
        let result = topological_sort(&g);
        assert!(!result.success);
        assert!(!result.cycle_nodes.is_empty());
    }

    #[test]
    fn reachability_detects_unreachable_and_dead_end_nodes() {
        let g = graph(
            vec![
                node("start", "START"),
                node("ok", "VARIABLE"),
                node("dead", "VARIABLE"),
                node("isolated", "VARIABLE"),
                node("end", "END"),
            ],
            vec![edge("start", "ok"), edge("ok", "end")],
            Some("start"),
            vec!["end"],
        );
        let result = analyze_reachability(&g);
        assert!(result.unreachable_nodes.contains(&"isolated".to_string()));
        assert!(result.unreachable_nodes.contains(&"dead".to_string()));
        assert!(!result.dead_end_nodes.contains(&"ok".to_string()));
    }

    #[test]
    fn reachability_reverse_traversal() {
        let g = graph(
            vec![
                node("start", "START"),
                node("a", "VARIABLE"),
                node("b", "VARIABLE"),
                node("end", "END"),
            ],
            vec![edge("start", "a"), edge("a", "b"), edge("b", "end")],
            Some("start"),
            vec!["end"],
        );
        let reaching = get_nodes_reaching_to(&g, "end");
        assert_eq!(reaching.len(), 4);
        let reachable = get_reachable_nodes(&g, "start");
        assert_eq!(reachable.len(), 4);
    }

    #[test]
    fn analysis_collects_stats() {
        let g = graph(
            vec![node("start", "START"), node("end", "END")],
            vec![edge("start", "end")],
            Some("start"),
            vec!["end"],
        );
        let analysis = analyze_graph(&g);
        assert_eq!(analysis.node_total, 2);
        assert_eq!(analysis.edge_total, 1);
        assert_eq!(analysis.node_counts_by_type.get("START"), Some(&1));
        assert!(!analysis.cycle_detection.has_cycle);
    }
}

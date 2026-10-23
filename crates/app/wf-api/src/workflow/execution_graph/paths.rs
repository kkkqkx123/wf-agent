use std::collections::{HashMap, HashSet, VecDeque};

use wf_types::workflow_execution::WorkflowGraphStructure;

use super::*;

pub(crate) fn dfs_paths(
    outgoing: &HashMap<&str, Vec<&str>>,
    start: &str,
    is_terminal: impl Fn(&str) -> bool,
) -> Vec<ExecutionPath> {
    let mut paths = Vec::new();
    let mut stack: Vec<(Vec<String>, HashSet<String>)> =
        vec![(vec![start.to_string()], HashSet::from([start.to_string()]))];
    while let Some((path, visited)) = stack.pop() {
        if paths.len() >= MAX_ENUMERATED_PATHS {
            break;
        }
        let current = path.last().expect("path is never empty");
        if is_terminal(current) {
            paths.push(ExecutionPath {
                nodes: path.clone(),
                length: path.len(),
            });
            continue;
        }
        let Some(next) = outgoing.get(current.as_str()) else {
            continue;
        };
        for target in next {
            if visited.contains(*target) {
                continue;
            }
            let mut next_path = path.clone();
            next_path.push((*target).to_string());
            let mut next_visited = visited.clone();
            next_visited.insert((*target).to_string());
            stack.push((next_path, next_visited));
        }
    }
    paths.sort_by(|a, b| {
        b.length
            .cmp(&a.length)
            .then_with(|| a.nodes.join(",").cmp(&b.nodes.join(",")))
    });
    paths
}

pub fn enumerate_paths(graph: &WorkflowGraphStructure) -> Vec<ExecutionPath> {
    let outgoing: HashMap<&str, Vec<&str>> =
        graph.edges.iter().fold(HashMap::new(), |mut acc, e| {
            acc.entry(e.source_node_id.as_str())
                .or_insert_with(Vec::new)
                .push(e.target_node_id.as_str());
            acc
        });

    let Some(start) = graph.start_node_id.as_deref() else {
        return Vec::new();
    };
    let end_set: HashSet<&str> = graph.end_node_ids.iter().map(String::as_str).collect();

    dfs_paths(&outgoing, start, |current| end_set.contains(current))
}

/// Identify branching nodes (multiple conditional outgoing edges) and the
/// alternative each took, based on the actual execution order.
pub fn analyze_decision_points(
    graph: &WorkflowGraphStructure,
    executed_nodes: &[String],
) -> Vec<DecisionPoint> {
    let execution_index: HashMap<&str, usize> = executed_nodes
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let mut points = Vec::new();
    for node in &graph.nodes {
        let outgoing: Vec<&wf_types::workflow_execution::WorkflowEdge> = graph
            .edges
            .iter()
            .filter(|e| e.source_node_id == node.id)
            .collect();
        let conditional: Vec<_> = outgoing.iter().filter(|e| e.condition.is_some()).collect();
        if conditional.len() < 2 {
            continue;
        }
        // Which alternative was taken: the first executed successor of the
        // branching node.
        let current_pos = execution_index.get(node.id.as_str()).copied();
        let taken_edge = current_pos.and_then(|pos| {
            executed_nodes
                .iter()
                .skip(pos + 1)
                .find(|id| outgoing.iter().any(|e| e.target_node_id == **id))
                .and_then(|target| {
                    outgoing
                        .iter()
                        .find(|e| e.target_node_id == *target)
                        .map(|e| e.id.clone())
                })
        });
        let untaken_conditions = conditional
            .iter()
            .filter(|e| taken_edge.as_deref() != Some(e.id.as_str()))
            .filter_map(|e| e.condition.clone())
            .collect();
        points.push(DecisionPoint {
            node_id: node.id.clone(),
            node_name: node.name.clone().unwrap_or_else(|| node.id.clone()),
            node_type: node.node_type.clone(),
            alternative_count: conditional.len(),
            taken_edge,
            untaken_conditions,
        });
    }
    points
}

/// Breadth-first reachability of an execution's graph from its start node.
pub fn reachable_nodes(graph: &WorkflowGraphStructure) -> Vec<String> {
    let Some(start) = graph.start_node_id.as_deref() else {
        return Vec::new();
    };
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([start.to_string()]);
    visited.insert(start.to_string());
    while let Some(current) = queue.pop_front() {
        for edge in &graph.edges {
            if edge.source_node_id == current && visited.insert(edge.target_node_id.clone()) {
                queue.push_back(edge.target_node_id.clone());
            }
        }
    }
    visited.into_iter().collect()
}

/// Small extension helper so an empty graph can be constructed easily.
pub(crate) trait EmptyGraph {
    fn default_empty() -> Self;
}

impl EmptyGraph for WorkflowGraphStructure {
    fn default_empty() -> Self {
        WorkflowGraphStructure {
            nodes: Vec::new(),
            edges: Vec::new(),
            start_node_id: None,
            end_node_ids: Vec::new(),
            error_default: None,
        }
    }
}

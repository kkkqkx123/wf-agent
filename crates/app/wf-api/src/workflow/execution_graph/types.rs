use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

pub const MAX_ENUMERATED_PATHS: usize = 1000;

/// One simple path from the start node to an end node.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPath {
    pub nodes: Vec<String>,
    pub length: usize,
}

/// Bounded DFS over an adjacency map, collecting every simple path from
/// `start` to a terminal node (no node repeated within a path). Shared by
/// the workflow and agent graph analyses; paths are sorted longest-first
/// with a stable node-sequence tie-break.
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

/// A node with multiple conditional outgoing edges and the edge it took.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionPoint {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    /// Number of outgoing conditional edges (alternatives).
    pub alternative_count: usize,
    /// The alternative actually taken during execution.
    pub taken_edge: Option<String>,
    /// The conditions of the untaken alternatives (for unexplored-branch
    /// analysis).
    pub untaken_conditions: Vec<String>,
}

/// Execution path analysis for one workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPathAnalysis {
    pub execution_id: String,
    pub workflow_id: Option<String>,
    /// Every structural path from start to an end node (bounded).
    pub paths: Vec<ExecutionPath>,
    /// The longest structural path (by node count).
    pub critical_path: ExecutionPath,
    /// Number of nodes per node type in the graph.
    pub node_type_distribution: BTreeMap<String, usize>,
    /// The actual node execution order (successful attempts, live entity).
    pub executed_nodes: Vec<String>,
    /// Branching nodes and the alternative they took.
    pub decision_points: Vec<DecisionPoint>,
    /// Node ids that exist in the graph but were not executed.
    pub unexecuted_nodes: Vec<String>,
}

/// One slow node above a duration percentile threshold.
#[derive(Debug, Clone, Serialize)]
pub struct SlowNodeView {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub duration_ms: i64,
    pub success: bool,
}

/// Efficiency analysis comparing the executed steps to the shortest
/// structural path.
#[derive(Debug, Clone, Serialize)]
pub struct EfficiencyAnalysis {
    pub execution_id: String,
    pub executed_steps: usize,
    pub optimal_steps: usize,
    /// executed / optimal (>= 1.0; larger = more wasteful).
    pub efficiency_ratio: f64,
    pub wasteful_nodes: usize,
    pub retry_count: usize,
}

/// An alternative branch considered but not taken at a decision point.
#[derive(Debug, Clone, Serialize)]
pub struct AlternativeDecision {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_probability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// One structural path with its estimated probability.
#[derive(Debug, Clone, Serialize)]
pub struct PathProbabilityEntry {
    pub path_id: String,
    pub node_ids: Vec<String>,
    pub probability: f64,
    pub is_taken: bool,
}

/// Path probability analysis of a workflow execution. Edge probabilities are
/// inferred: conditional outgoing edges of a branching node share its
/// probability mass uniformly.
#[derive(Debug, Clone, Serialize)]
pub struct PathProbabilityAnalysis {
    pub execution_id: String,
    pub paths: Vec<PathProbabilityEntry>,
    pub most_likely_path: Option<Vec<String>>,
    pub least_likely_taken_path: Option<Vec<String>>,
    /// Normalized entropy of the path probabilities (0.0 - 1.0).
    pub path_diversity: f64,
}

/// Normalized timing of one node execution.
#[derive(Debug, Clone)]
pub struct NodeTiming {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub duration_ms: i64,
    pub success: bool,
}

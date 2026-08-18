//! Execution graph / decision-point analysis.
//!
//! Pure query functions over the persisted execution record (graph structure
//! plus status) combined with the live entity's node execution history.
//!
//! Capabilities: execution path enumeration (DFS), critical-path detection,
//! node-type distribution, and decision-point analysis that reports which
//! conditional edge a branching node actually took.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};
use crate::infra::util::round2;
use crate::workflow::workflow_execution::definition_to_graph;

/// Upper bound on enumerated paths to keep DFS bounded on dense graphs.
/// Single shared constant for every path enumeration in the crate.
const MAX_ENUMERATED_PATHS: usize = 1000;

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

/// Enumerate execution paths, critical path, node-type distribution and
/// decision points for an execution. Degrades gracefully to an empty
/// analysis when neither the live entity nor a persisted graph is
/// available.
pub async fn analyze(ctx: &ApiContext, execution_id: &str) -> ApiResult<ExecutionPathAnalysis> {
    let workflow_id = if let Some(entity) = ctx.workflow_execution(execution_id) {
        Some(entity.workflow_id().to_string())
    } else if let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? {
        Some(record.workflow_id.to_string())
    } else {
        None
    };

    let graph = resolve_graph(ctx, execution_id)
        .await?
        .unwrap_or_else(WorkflowGraphStructure::default_empty);
    let paths = enumerate_paths(&graph);
    let critical_path = paths
        .iter()
        .max_by_key(|path| path.length)
        .cloned()
        .unwrap_or(ExecutionPath {
            nodes: Vec::new(),
            length: 0,
        });

    let mut node_type_distribution = BTreeMap::new();
    for node in &graph.nodes {
        *node_type_distribution
            .entry(node.node_type.clone())
            .or_insert(0) += 1;
    }

    let executed_nodes = executed_nodes(ctx, execution_id).await;
    let decision_points = analyze_decision_points(&graph, &executed_nodes);

    let executed_set: HashSet<&str> = executed_nodes.iter().map(String::as_str).collect();
    let unexecuted_nodes = graph
        .nodes
        .iter()
        .map(|n| n.id.clone())
        .filter(|id| !executed_set.contains(id.as_str()))
        .collect();

    Ok(ExecutionPathAnalysis {
        execution_id: execution_id.to_string(),
        workflow_id,
        paths,
        critical_path,
        node_type_distribution,
        executed_nodes,
        decision_points,
        unexecuted_nodes,
    })
}

/// Resolve the graph of an execution: the live entity has none, the
/// persisted record may carry one, otherwise the workflow definition is
/// converted (flat template semantics).
pub(crate) async fn resolve_graph(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<WorkflowGraphStructure>> {
    if let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? {
        if let Some(graph) = record.graph {
            return Ok(Some(graph));
        }
        let definition = ctx
            .storage
            .workflow
            .load(&record.workflow_id)
            .await?
            .ok_or_else(|| not_found("workflow", &record.workflow_id))?;
        return Ok(Some(definition_to_graph(&definition)));
    }
    Ok(None)
}

/// Actual execution order from the live entity's node execution history
/// (successful attempts only, in start-time order), falling back to the
/// persisted record's `node_results` after a restart.
async fn executed_nodes(ctx: &ApiContext, execution_id: &str) -> Vec<String> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        let mut records = state.node_execution_history().to_vec();
        records.sort_by_key(|r| r.start_time);
        return records
            .into_iter()
            .filter(|r| r.success)
            .map(|r| r.node_id)
            .collect();
    }
    let Ok(Some(record)) = ctx.storage.workflow_execution.load(execution_id).await else {
        return Vec::new();
    };
    let mut results = record.node_results.unwrap_or_default();
    results.sort_by_key(|r| r.started_at.unwrap_or(0));
    results
        .into_iter()
        .filter(|r| r.status == "completed")
        .map(|r| r.node_id)
        .collect()
}

/// Node timing records of an execution: the live entity's node execution
/// history, otherwise the persisted record's `node_results`.
async fn node_timings(ctx: &ApiContext, execution_id: &str) -> Vec<NodeTiming> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        let mut records: Vec<_> = state
            .node_execution_history()
            .iter()
            .map(|r| NodeTiming {
                node_id: r.node_id.clone(),
                node_name: r.node_name.clone(),
                node_type: r.node_type.clone(),
                duration_ms: r
                    .end_time
                    .map(|end| (end - r.start_time).max(0))
                    .unwrap_or(0),
                success: r.success,
            })
            .collect();
        records.sort_by_key(|r| r.node_id.clone());
        return records;
    }
    let Ok(Some(record)) = ctx.storage.workflow_execution.load(execution_id).await else {
        return Vec::new();
    };
    record
        .node_results
        .unwrap_or_default()
        .into_iter()
        .map(|result| NodeTiming {
            node_id: result.node_id.clone(),
            node_name: result.node_id.clone(),
            node_type: "unknown".to_string(),
            duration_ms: result
                .completed_at
                .zip(result.started_at)
                .map(|(end, start)| (end - start).max(0))
                .unwrap_or(0),
            success: result.status == "completed",
        })
        .collect()
}

/// Resolve the graph of an execution and record it onto the persisted
/// `WorkflowExecution` record so the real per-execution graph survives a
/// restart.
pub async fn record_execution_graph(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowGraphStructure> {
    let graph = resolve_graph(ctx, execution_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    if let Some(mut record) = ctx.storage.workflow_execution.load(execution_id).await? {
        record.graph = Some(graph.clone());
        ctx.storage.workflow_execution.save(&record).await?;
    }
    Ok(graph)
}

/// Nodes whose duration ranks in the slowest `(1 - percentile)` fraction
/// of the execution (default percentile 0.8 = slowest 20%). Duration
/// comparisons are over positive durations only.
pub async fn get_slow_nodes(
    ctx: &ApiContext,
    execution_id: &str,
    percentile: f64,
) -> ApiResult<Vec<SlowNodeView>> {
    let timings = node_timings(ctx, execution_id).await;
    let percentile = percentile.clamp(0.0, 1.0);
    let mut candidates: Vec<NodeTiming> =
        timings.into_iter().filter(|t| t.duration_ms > 0).collect();
    candidates.sort_by(|a, b| {
        b.duration_ms
            .cmp(&a.duration_ms)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let keep = ((candidates.len() as f64) * (1.0 - percentile))
        .ceil()
        .max(1.0) as usize;
    Ok(candidates
        .into_iter()
        .take(keep)
        .map(|t| SlowNodeView {
            node_id: t.node_id,
            node_name: t.node_name,
            node_type: t.node_type,
            duration_ms: t.duration_ms,
            success: t.success,
        })
        .collect())
}

/// Efficiency of the execution relative to the shortest structural path
/// through the graph.
pub async fn analyze_efficiency(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<EfficiencyAnalysis> {
    let graph = resolve_graph(ctx, execution_id)
        .await?
        .unwrap_or_else(WorkflowGraphStructure::default_empty);
    let paths = enumerate_paths(&graph);
    let optimal_steps = paths
        .iter()
        .map(|p| p.length)
        .min()
        .unwrap_or(graph.nodes.len().max(1));

    let timings = node_timings(ctx, execution_id).await;
    let executed_steps = timings.iter().filter(|t| t.success).count();
    let retry_count = timings.iter().filter(|t| !t.success).count();

    let wasteful_nodes = executed_steps.saturating_sub(optimal_steps);
    let efficiency_ratio = if optimal_steps > 0 {
        round2(executed_steps as f64 / optimal_steps as f64)
    } else {
        0.0
    };

    Ok(EfficiencyAnalysis {
        execution_id: execution_id.to_string(),
        executed_steps,
        optimal_steps,
        efficiency_ratio,
        wasteful_nodes,
        retry_count,
    })
}

/// Alternative branches that exist at decision points but were not taken
/// during this execution.
pub async fn get_alternative_paths(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<AlternativeDecision>> {
    let graph = resolve_graph(ctx, execution_id)
        .await?
        .unwrap_or_else(WorkflowGraphStructure::default_empty);
    let executed_nodes = executed_nodes(ctx, execution_id).await;
    let decision_points = analyze_decision_points(&graph, &executed_nodes);

    let mut alternatives = Vec::new();
    for point in &decision_points {
        let taken = point.taken_edge.clone();
        for edge in &graph.edges {
            if edge.source_node_id != point.node_id {
                continue;
            }
            if Some(edge.id.as_str()) == taken.as_deref() {
                continue;
            }
            let target_name = graph
                .nodes
                .iter()
                .find(|n| n.id == edge.target_node_id)
                .and_then(|n| n.name.clone())
                .unwrap_or_else(|| edge.target_node_id.clone());
            alternatives.push(AlternativeDecision {
                node_id: edge.target_node_id.clone(),
                node_name: Some(target_name.clone()),
                description: format!(
                    "Path via {} through edge '{}'",
                    target_name,
                    edge.label.clone().unwrap_or_else(|| edge.id.clone())
                ),
                reason: edge.condition.clone(),
                success_probability: None,
                confidence: None,
            });
        }
    }
    alternatives.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    Ok(alternatives)
}

/// Probability analysis over all structural paths of the execution graph.
pub async fn get_path_probability_analysis(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<PathProbabilityAnalysis> {
    let graph = resolve_graph(ctx, execution_id)
        .await?
        .unwrap_or_else(WorkflowGraphStructure::default_empty);
    let paths = enumerate_paths(&graph);
    if paths.is_empty() {
        return Ok(PathProbabilityAnalysis {
            execution_id: execution_id.to_string(),
            paths: Vec::new(),
            most_likely_path: None,
            least_likely_taken_path: None,
            path_diversity: 0.0,
        });
    }

    let executed_nodes = executed_nodes(ctx, execution_id).await;
    let executed_set: HashSet<&str> = executed_nodes.iter().map(String::as_str).collect();
    let edge_probability = edge_probabilities(&graph);

    let mut entries: Vec<PathProbabilityEntry> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let mut probability = 1.0;
            for window in path.nodes.windows(2) {
                let source = &window[0];
                let target = &window[1];
                probability *= edge_probability
                    .get(&(source.clone(), target.clone()))
                    .copied()
                    .unwrap_or(1.0);
            }
            let is_taken = path
                .nodes
                .iter()
                .all(|id| executed_set.contains(id.as_str()));
            PathProbabilityEntry {
                path_id: format!("path-{index}"),
                node_ids: path.nodes.clone(),
                probability: round3(probability),
                is_taken,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.probability
            .partial_cmp(&a.probability)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let most_likely_path = entries.first().map(|e| e.node_ids.clone());
    let taken_paths: Vec<&PathProbabilityEntry> = entries.iter().filter(|e| e.is_taken).collect();
    let least_likely_taken_path = taken_paths
        .iter()
        .min_by(|a, b| {
            a.probability
                .partial_cmp(&b.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|e| e.node_ids.clone());

    let total_probability: f64 = entries.iter().map(|e| e.probability).sum();
    let path_diversity = if total_probability > 0.0 && entries.len() > 1 {
        let entropy = -entries
            .iter()
            .map(|e| {
                let normalized = e.probability / total_probability;
                if normalized > 0.0 {
                    normalized * normalized.log2()
                } else {
                    0.0
                }
            })
            .sum::<f64>();
        round3(entropy / (entries.len() as f64).log2())
    } else {
        0.0
    };

    Ok(PathProbabilityAnalysis {
        execution_id: execution_id.to_string(),
        paths: entries,
        most_likely_path,
        least_likely_taken_path,
        path_diversity,
    })
}

/// Clear the recorded execution data (persisted graph) of an execution.
pub async fn clear_execution_data(ctx: &ApiContext, execution_id: &str) -> ApiResult<()> {
    if let Some(mut record) = ctx.storage.workflow_execution.load(execution_id).await? {
        record.graph = None;
        ctx.storage.workflow_execution.save(&record).await?;
    }
    Ok(())
}

/// Normalized timing of one node execution.
#[derive(Debug, Clone)]
struct NodeTiming {
    node_id: String,
    node_name: String,
    node_type: String,
    duration_ms: i64,
    success: bool,
}

/// Estimated probability of each directed edge. A node with `k` conditional
/// outgoing edges splits its probability mass evenly (`1/k`); unconditional
/// edges keep `1.0`.
fn edge_probabilities(graph: &WorkflowGraphStructure) -> HashMap<(String, String), f64> {
    let mut probabilities = HashMap::new();
    for node in &graph.nodes {
        let conditional: Vec<&wf_types::workflow_execution::WorkflowEdge> = graph
            .edges
            .iter()
            .filter(|e| e.source_node_id == node.id && e.condition.is_some())
            .collect();
        let share = if conditional.is_empty() {
            1.0
        } else {
            1.0 / conditional.len() as f64
        };
        for edge in graph.edges.iter().filter(|e| e.source_node_id == node.id) {
            probabilities.insert(
                (edge.source_node_id.clone(), edge.target_node_id.clone()),
                share,
            );
        }
    }
    probabilities
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Enumerate all start-to-end paths of the graph via DFS (bounded).
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
trait EmptyGraph {
    fn default_empty() -> Self;
}

impl EmptyGraph for WorkflowGraphStructure {
    fn default_empty() -> Self {
        WorkflowGraphStructure {
            nodes: Vec::new(),
            edges: Vec::new(),
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: None,
            end_node_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::workflow::edge::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    fn node(id: &str, node_type: &str) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner: serde_json::json!({}),
        }
    }

    fn edge(source: &str, target: &str, condition: Option<&str>) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{}-{}", source, target),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: condition.map(String::from),
            label: None,
            description: None,
        }
    }

    fn graph() -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes: vec![
                node("start", "START"),
                node("route", "ROUTE"),
                node("a", "VARIABLE"),
                node("b", "VARIABLE"),
                node("end", "END"),
            ],
            edges: vec![
                edge("start", "route", None),
                edge("route", "a", Some("${input.x} > 0")),
                edge("route", "b", Some("${input.x} <= 0")),
                edge("a", "end", None),
                edge("b", "end", None),
            ],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        }
    }

    #[test]
    fn enumerates_all_paths() {
        let paths = enumerate_paths(&graph());
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.nodes.contains(&"a".to_string())));
        assert!(paths.iter().any(|p| p.nodes.contains(&"b".to_string())));
        // Critical path length is identical here (start->route->a->end).
        assert_eq!(paths[0].length, 4);
    }

    #[test]
    fn decision_points_detect_taken_alternative() {
        let graph = graph();
        let executed: Vec<String> = vec!["start", "route", "b", "end"]
            .into_iter()
            .map(String::from)
            .collect();
        let points = analyze_decision_points(&graph, &executed);
        assert_eq!(points.len(), 1);
        let point = &points[0];
        assert_eq!(point.node_id, "route");
        assert_eq!(point.alternative_count, 2);
        assert_eq!(point.taken_edge.as_deref(), Some("route-b"));
        assert_eq!(point.untaken_conditions, vec!["${input.x} > 0".to_string()]);
    }

    #[tokio::test]
    async fn analyze_degrades_gracefully() {
        use std::sync::Arc;

        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ));
        // No live entity and no persisted record: empty analysis, no error.
        let analysis = analyze(&ctx, "missing-exec").await.unwrap();
        assert!(analysis.paths.is_empty());
        assert!(analysis.executed_nodes.is_empty());
    }

    async fn persist(ctx: &ApiContext, execution_id: &str, workflow_id: &str) {
        let record = wf_types::WorkflowExecution {
            id: execution_id.into(),
            workflow_id: workflow_id.into(),
            workflow_version: None,
            status: wf_types::ExecutionStatus::Completed,
            current_node_id: None,
            graph: Some(graph()),
            variables: None,
            input: None,
            output: None,
            node_results: Some(vec![
                wf_types::workflow_execution::NodeExecutionResult {
                    node_id: "start".into(),
                    status: "completed".into(),
                    input: None,
                    output: None,
                    error: None,
                    started_at: Some(1000),
                    completed_at: Some(1100),
                    retry_count: 0,
                },
                wf_types::workflow_execution::NodeExecutionResult {
                    node_id: "route".into(),
                    status: "completed".into(),
                    input: None,
                    output: None,
                    error: None,
                    started_at: Some(1100),
                    completed_at: Some(1200),
                    retry_count: 0,
                },
                wf_types::workflow_execution::NodeExecutionResult {
                    node_id: "b".into(),
                    status: "completed".into(),
                    input: None,
                    output: None,
                    error: None,
                    started_at: Some(1200),
                    completed_at: Some(1500),
                    retry_count: 1,
                },
                wf_types::workflow_execution::NodeExecutionResult {
                    node_id: "b".into(),
                    status: "completed".into(),
                    input: None,
                    output: None,
                    error: None,
                    started_at: Some(1500),
                    completed_at: Some(3500),
                    retry_count: 0,
                },
                wf_types::workflow_execution::NodeExecutionResult {
                    node_id: "end".into(),
                    status: "completed".into(),
                    input: None,
                    output: None,
                    error: None,
                    started_at: Some(3500),
                    completed_at: Some(3600),
                    retry_count: 0,
                },
            ]),
            errors: None,
            error: None,
            started_at: 1000,
            completed_at: Some(3600),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();
    }

    #[tokio::test]
    async fn record_and_clear_execution_graph() {
        use std::sync::Arc;

        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ));
        persist(&ctx, "exec-g", "wf-g").await;

        let recorded = record_execution_graph(&ctx, "exec-g").await.unwrap();
        assert_eq!(recorded.nodes.len(), 5);

        let record = ctx
            .storage
            .workflow_execution
            .load("exec-g")
            .await
            .unwrap()
            .unwrap();
        assert!(record.graph.is_some());

        clear_execution_data(&ctx, "exec-g").await.unwrap();
        let record = ctx
            .storage
            .workflow_execution
            .load("exec-g")
            .await
            .unwrap()
            .unwrap();
        assert!(record.graph.is_none());
    }

    #[tokio::test]
    async fn slow_nodes_and_efficiency() {
        use std::sync::Arc;

        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ));
        persist(&ctx, "exec-eff", "wf-eff").await;

        let slow = get_slow_nodes(&ctx, "exec-eff", 0.8).await.unwrap();
        assert!(
            !slow.is_empty(),
            "b (2000ms) should exceed the 80th percentile"
        );
        assert!(slow.iter().any(|s| s.node_id == "b"));
        assert!(slow.iter().all(|s| s.duration_ms > 0));

        let efficiency = analyze_efficiency(&ctx, "exec-eff").await.unwrap();
        // 5 successful executions across 4 distinct nodes; shortest path = 4.
        assert!(efficiency.optimal_steps >= 1);
        assert_eq!(efficiency.executed_steps, 5);
        assert!(efficiency.efficiency_ratio >= 1.0);
        assert_eq!(efficiency.retry_count, 0, "persisted results all completed");
    }

    #[tokio::test]
    async fn alternative_paths_and_probability() {
        use std::sync::Arc;

        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ));
        persist(&ctx, "exec-prob", "wf-prob").await;

        let alternatives = get_alternative_paths(&ctx, "exec-prob").await.unwrap();
        assert_eq!(alternatives.len(), 1, "the untaken 'route -> a' branch");
        assert_eq!(alternatives[0].node_id, "a");

        let probability = get_path_probability_analysis(&ctx, "exec-prob")
            .await
            .unwrap();
        assert_eq!(probability.paths.len(), 2);
        for path in &probability.paths {
            assert!(
                (path.probability - 0.5).abs() < 0.001,
                "uniform branch split"
            );
        }
        assert!(probability.most_likely_path.is_some());
        assert!(probability.path_diversity >= 0.0);
        assert!(probability.paths.iter().any(|p| p.is_taken));
    }
}

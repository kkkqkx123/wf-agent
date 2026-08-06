//! Execution graph / decision-point analysis (TS
//! `WorkflowExecutionGraphQueryAPI` counterpart).
//!
//! Pure query functions over the persisted execution record (graph structure
//! plus status) combined with the live entity's node execution history.
//!
//! Capabilities: execution path enumeration (DFS), critical-path detection,
//! node-type distribution, and decision-point analysis that reports which
//! conditional edge a branching node actually took.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};
use crate::workflow_execution::definition_to_graph;

/// Upper bound on enumerated paths to keep DFS bounded on dense graphs.
const MAX_ENUMERATED_PATHS: usize = 1000;

/// One simple path from the start node to an end node.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPath {
    pub nodes: Vec<String>,
    pub length: usize,
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

/// Execution-graph queries over workflow executions.
pub struct ExecutionGraphApi {
    ctx: Arc<ApiContext>,
}

impl ExecutionGraphApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Enumerate execution paths, critical path, node-type distribution and
    /// decision points for an execution. Degrades gracefully to an empty
    /// analysis when neither the live entity nor a persisted graph is
    /// available.
    pub async fn analyze(&self, execution_id: &str) -> ApiResult<ExecutionPathAnalysis> {
        let workflow_id = if let Some(entity) = self.ctx.workflow_execution(execution_id) {
            Some(entity.workflow_id().to_string())
        } else if let Some(record) = self
            .ctx
            .storage
            .workflow_execution
            .load(execution_id)
            .await?
        {
            Some(record.workflow_id.to_string())
        } else {
            None
        };

        let graph = self
            .resolve_graph(execution_id)
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

        let executed_nodes = self.executed_nodes(execution_id).await;
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
    async fn resolve_graph(&self, execution_id: &str) -> ApiResult<Option<WorkflowGraphStructure>> {
        if let Some(record) = self
            .ctx
            .storage
            .workflow_execution
            .load(execution_id)
            .await?
        {
            if let Some(graph) = record.graph {
                return Ok(Some(graph));
            }
            let definition = self
                .ctx
                .storage
                .workflow
                .load(&record.workflow_id)
                .await?
                .ok_or_else(|| ApiError::not_found("workflow", &record.workflow_id))?;
            return Ok(Some(definition_to_graph(&definition)));
        }
        Ok(None)
    }

    /// Actual execution order from the live entity's node execution history
    /// (successful attempts only, in start-time order).
    async fn executed_nodes(&self, execution_id: &str) -> Vec<String> {
        let Some(entity) = self.ctx.workflow_execution(execution_id) else {
            return Vec::new();
        };
        let state = entity.state.read().await;
        let mut records = state.node_execution_history().to_vec();
        records.sort_by_key(|r| r.start_time);
        records
            .into_iter()
            .filter(|r| r.success)
            .map(|r| r.node_id)
            .collect()
    }
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

    let mut paths = Vec::new();
    let mut stack: Vec<(Vec<String>, HashSet<String>)> =
        vec![(vec![start.to_string()], HashSet::from([start.to_string()]))];
    while let Some((path, visited)) = stack.pop() {
        if paths.len() >= MAX_ENUMERATED_PATHS {
            break;
        }
        let current = path.last().expect("path is never empty");
        if end_set.contains(current.as_str()) {
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
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
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
        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ));
        let api = ExecutionGraphApi::new(ctx);
        // No live entity and no persisted record: empty analysis, no error.
        let analysis = api.analyze("missing-exec").await.unwrap();
        assert!(analysis.paths.is_empty());
        assert!(analysis.executed_nodes.is_empty());
    }
}

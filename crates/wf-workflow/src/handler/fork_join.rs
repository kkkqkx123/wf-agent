use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;
use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::{ExecutorContext, NodeExecutionContext, NodeExecutionResult};
use wf_tools::registry::ToolRegistry;
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};

use crate::barrier::{BranchResult, FailureStrategy, ForkOutcome, SyncBarrier};
use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

fn resolve_handlers(
    ctx: &NodeExecutionContext,
) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    match &ctx.handler_registry {
        Some(any) => match any
            .clone()
            .downcast::<HashMap<StaticNodeType, Box<dyn NodeHandler>>>()
        {
            Ok(handlers) => handlers,
            Err(_) => Arc::new(HashMap::new()),
        },
        None => Arc::new(HashMap::new()),
    }
}

fn emit_fork_event(
    event_bus: Option<&Arc<EventBus>>,
    event_type: EventType,
    execution_id: &wf_types::Id,
    metadata: HashMap<String, Value>,
) {
    let Some(bus) = event_bus else { return };
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.clone()),
        agent_loop_id: None,
        metadata: Some(metadata),
    };
    let _ = bus.publish(event);
}

/// Extract the subgraph reachable from a fork branch edge up to (and
/// including) the join node. Uses BFS so nested forks, parallel sub-branches
/// and converging paths inside the branch are all collected; traversal stops
/// at the join node, so it is not expanded past.
fn extract_branch_subgraph(
    graph: &WorkflowGraphStructure,
    fork_node_id: &str,
    branch_edge: &WorkflowEdge,
    join_node_id: &str,
) -> WorkflowGraphStructure {
    let mut branch_nodes: HashSet<String> = HashSet::new();
    let mut branch_edges: Vec<WorkflowEdge> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();

    let edge_map: HashMap<&str, Vec<&WorkflowEdge>> =
        graph.edges.iter().fold(HashMap::new(), |mut acc, e| {
            acc.entry(e.source_node_id.as_str())
                .or_insert_with(Vec::new)
                .push(e);
            acc
        });

    branch_nodes.insert(fork_node_id.to_string());
    branch_nodes.insert(branch_edge.target_node_id.clone());
    queue.push_back(branch_edge.target_node_id.clone());

    while let Some(current) = queue.pop_front() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if current == join_node_id {
            continue;
        }
        if let Some(edges) = edge_map.get(current.as_str()) {
            for edge in edges {
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

    let end_node_ids: Vec<String> = nodes
        .iter()
        .map(|n| n.id.clone())
        .filter(|nid| {
            nid == join_node_id || edge_map.get(nid.as_str()).is_none_or(|es| es.is_empty())
        })
        .collect();

    WorkflowGraphStructure {
        nodes,
        edges: branch_edges,
        start_node_id: Some(branch_edge.target_node_id.clone()),
        end_node_ids,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
    }
}

/// Find the JOIN node whose `fork_path_ids` equal the fork's `path_ids`.
fn find_join_node(graph: &WorkflowGraphStructure, path_ids: &[String]) -> Option<String> {
    graph
        .nodes
        .iter()
        .find(|n| {
            n.node_type == "JOIN"
                && n.inner
                    .get("fork_path_ids")
                    .and_then(|v| v.as_array())
                    .map(|ids| {
                        ids.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()
                            == path_ids.iter().map(String::as_str).collect::<Vec<_>>()
                    })
                    .unwrap_or(false)
        })
        .map(|n| n.id.clone())
}

pub struct ForkHandler;

#[async_trait]
impl NodeHandler for ForkHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Fork
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let paths = config
            .get("fork_paths")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        if paths.is_empty() {
            return Err(WorkflowError::ForkJoinError(
                "No fork_paths defined for fork node".to_string(),
            ));
        }

        let failure_strategy = config
            .get("failure_strategy")
            .and_then(|s| s.as_str())
            .and_then(|s| match s {
                "fail_fast" => Some(FailureStrategy::FailFast),
                "continue_on_error" => Some(FailureStrategy::ContinueOnError),
                "fail_on_threshold" => {
                    let threshold = config
                        .get("failure_threshold")
                        .and_then(|t| t.as_f64())
                        .unwrap_or(0.5);
                    Some(FailureStrategy::FailOnThreshold { threshold })
                }
                _ => None,
            })
            .unwrap_or(FailureStrategy::FailFast);

        let fork_strategy = config
            .get("fork_strategy")
            .and_then(|s| s.as_str())
            .unwrap_or("parallel");

        let event_bus = ctx.event_bus.clone();
        let execution_id = ctx.execution_id.clone();
        let node_id = ctx.node_id.clone();

        emit_fork_event(
            event_bus.as_ref(),
            EventType::ForkStarted,
            &execution_id,
            HashMap::from([
                (
                    "branch_count".to_string(),
                    Value::Number(serde_json::Number::from(paths.len() as u64)),
                ),
                ("node_id".to_string(), Value::String(node_id.clone())),
            ]),
        );

        let graph: Option<WorkflowGraphStructure> = ctx
            .graph_structure
            .as_ref()
            .and_then(|any| any.downcast_ref::<WorkflowGraphStructure>())
            .cloned();

        let handlers = resolve_handlers(ctx);
        let path_ids: Vec<String> = paths
            .iter()
            .filter_map(|p| p.get("path_id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        // The JOIN node is the one whose fork_path_ids match this fork's
        // path ids.
        let join_node_id = graph.as_ref().and_then(|g| find_join_node(g, &path_ids));
        let tool_registry = ctx.tool_registry.clone();
        let parent_variables = ctx.variables.clone();

        let barrier = Arc::new(SyncBarrier::new(paths.len()));
        let event_bus_clone = event_bus.clone();
        let execution_id_clone = execution_id.clone();
        let node_id_clone = node_id.clone();
        let branch_input = ctx.input.clone();

        let results: Vec<BranchResult> = if fork_strategy == "serial" {
            let mut results = Vec::new();
            for (idx, path) in paths.iter().enumerate() {
                emit_fork_event(
                    event_bus.as_ref(),
                    EventType::ForkBranchStarted,
                    &execution_id,
                    HashMap::from([
                        (
                            "branch_id".to_string(),
                            Value::String(
                                path.get("path_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("path")
                                    .to_string(),
                            ),
                        ),
                        (
                            "branch_index".to_string(),
                            Value::Number(serde_json::Number::from(idx as u64)),
                        ),
                        ("node_id".to_string(), Value::String(node_id.clone())),
                    ]),
                );
                results.push(
                    run_branch(
                        idx,
                        path.clone(),
                        barrier.clone(),
                        event_bus_clone.clone(),
                        execution_id_clone.clone(),
                        node_id_clone.clone(),
                        handlers.clone(),
                        graph.clone(),
                        join_node_id.clone(),
                        tool_registry.clone(),
                        parent_variables.clone(),
                        branch_input.clone(),
                    )
                    .await,
                );
            }
            results
        } else {
            let mut handles = Vec::new();
            for (idx, path) in paths.iter().enumerate() {
                emit_fork_event(
                    event_bus.as_ref(),
                    EventType::ForkBranchStarted,
                    &execution_id,
                    HashMap::from([
                        (
                            "branch_id".to_string(),
                            Value::String(
                                path.get("path_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("path")
                                    .to_string(),
                            ),
                        ),
                        (
                            "branch_index".to_string(),
                            Value::Number(serde_json::Number::from(idx as u64)),
                        ),
                        ("node_id".to_string(), Value::String(node_id.clone())),
                    ]),
                );
                let handle = tokio::spawn(run_branch(
                    idx,
                    path.clone(),
                    barrier.clone(),
                    event_bus_clone.clone(),
                    execution_id_clone.clone(),
                    node_id_clone.clone(),
                    handlers.clone(),
                    graph.clone(),
                    join_node_id.clone(),
                    tool_registry.clone(),
                    parent_variables.clone(),
                    branch_input.clone(),
                ));
                handles.push(handle);
            }
            join_all(handles)
                .await
                .into_iter()
                .filter_map(|r| r.ok())
                .collect()
        };

        let outcome = failure_strategy.evaluate(&results);

        for result in &results {
            emit_fork_event(
                event_bus.as_ref(),
                EventType::ForkBranchCompleted,
                &execution_id,
                HashMap::from([
                    (
                        "branch_id".to_string(),
                        Value::String(result.branch_id.clone()),
                    ),
                    ("success".to_string(), Value::Bool(result.success)),
                ]),
            );
        }

        emit_fork_event(
            event_bus.as_ref(),
            EventType::ForkCompleted,
            &execution_id,
            HashMap::from([
                (
                    "branch_count".to_string(),
                    Value::Number(serde_json::Number::from(paths.len() as u64)),
                ),
                (
                    "success_count".to_string(),
                    Value::Number(serde_json::Number::from(
                        results.iter().filter(|r| r.success).count() as u64,
                    )),
                ),
                (
                    "outcome".to_string(),
                    Value::String(format!("{:?}", outcome)),
                ),
            ]),
        );

        let mut metadata = HashMap::new();
        metadata.insert(
            "branch_count".to_string(),
            Value::Number(serde_json::Number::from(paths.len() as u64)),
        );
        metadata.insert(
            "success_count".to_string(),
            Value::Number(serde_json::Number::from(
                results.iter().filter(|r| r.success).count() as u64,
            )),
        );
        metadata.insert(
            "outcome".to_string(),
            Value::String(format!("{:?}", outcome)),
        );

        let mut next_nodes: Vec<String> = Vec::new();
        if outcome != ForkOutcome::Failed {
            if let Some(target) = &join_node_id {
                next_nodes.push(target.clone());
            }
        }

        let outputs: Vec<Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "branch_id": r.branch_id,
                    "output": r.output,
                    "success": r.success,
                })
            })
            .collect();

        let output = serde_json::json!({
            "results": results,
            "outputs": outputs,
            "outcome": format!("{:?}", outcome),
        });

        Ok(NodeExecutionResult {
            output,
            next_node_ids: next_nodes,
            metadata,
        })
    }
}

struct BranchContext {
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    event_bus: Option<Arc<EventBus>>,
    tool_registry: Option<Arc<ToolRegistry>>,
    parent_variables: Arc<dashmap::DashMap<String, Value>>,
}

/// Execute one fork branch: extract the branch subgraph from the edge
/// carrying the path label and run it up to the join node.
#[allow(clippy::too_many_arguments)]
async fn run_branch(
    idx: usize,
    path: Value,
    barrier: Arc<SyncBarrier>,
    event_bus: Option<Arc<EventBus>>,
    execution_id: wf_types::Id,
    node_id: String,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    graph: Option<WorkflowGraphStructure>,
    join_node_id: Option<String>,
    tool_registry: Option<Arc<ToolRegistry>>,
    parent_variables: Arc<dashmap::DashMap<String, Value>>,
    branch_input: Value,
) -> BranchResult {
    let path_id = path
        .get("path_id")
        .and_then(|v| v.as_str())
        .unwrap_or("path")
        .to_string();
    let child_node_id = path
        .get("child_node_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let result = match &graph {
        Some(g) => {
            let outgoing: Vec<&WorkflowEdge> = g
                .edges
                .iter()
                .filter(|e| e.source_node_id == node_id)
                .collect();

            let branch_edge = outgoing
                .iter()
                .find(|e| e.label.as_deref() == Some(&path_id) || e.target_node_id == child_node_id)
                .or_else(|| outgoing.get(idx))
                .or_else(|| outgoing.first());

            match branch_edge {
                Some(edge) => {
                    let join_target = join_node_id.clone().unwrap_or_default();
                    let subgraph = extract_branch_subgraph(g, &node_id, edge, &join_target);

                    if subgraph.nodes.is_empty() {
                        BranchResult::success(&path_id, branch_input)
                    } else {
                        match execute_branch(
                            &execution_id,
                            &path_id,
                            branch_input,
                            subgraph,
                            BranchContext {
                                handlers,
                                event_bus,
                                tool_registry,
                                parent_variables,
                            },
                        )
                        .await
                        {
                            Ok(output) => output,
                            Err(e) => BranchResult::failure(&path_id, e.to_string()),
                        }
                    }
                }
                None => BranchResult::success(&path_id, branch_input),
            }
        }
        None => BranchResult::success(&path_id, branch_input),
    };
    barrier.notify_branch_completed(&path_id).await;
    result
}

async fn execute_branch(
    parent_execution_id: &wf_types::Id,
    branch_id: &str,
    input: Value,
    subgraph: WorkflowGraphStructure,
    branch_ctx: BranchContext,
) -> WorkflowResult<BranchResult> {
    let execution_id = wf_types::Id::new();
    let workflow_id = wf_types::Id::new();

    let options = WorkflowExecutionOptions {
        input: Some(input),
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(false),
        node_timeout: None,
        max_pause_duration: None,
        retry_budget: None,
        on_failure: None,
        max_retries: None,
        retry_delay_ms: None,
        exponential_backoff: None,
        fallback_output: None,
    };

    let tool_registry = branch_ctx
        .tool_registry
        .unwrap_or_else(|| Arc::new(ToolRegistry::new()));
    let exec_ctx = ExecutorContext::new(
        execution_id.clone(),
        workflow_id.clone(),
        branch_ctx.event_bus,
        tool_registry,
        options,
    )
    .with_parent_execution(parent_execution_id.clone());
    // Branches inherit a read-only snapshot of the parent variables.
    crate::handler::variable_mapping::inherit_all_variables(
        &branch_ctx.parent_variables,
        &exec_ctx.variables,
    );

    let entity = WorkflowExecutionEntity::new(execution_id.clone(), workflow_id);

    let mut coordinator: WorkflowCoordinator =
        WorkflowCoordinator::new(exec_ctx, subgraph, branch_ctx.handlers)?.with_entity(entity);

    match coordinator.execute().await {
        Ok(output) => Ok(BranchResult::success(branch_id, output)),
        Err(e) => Ok(BranchResult::failure(branch_id, e.to_string())),
    }
}

pub struct JoinHandler;

/// Extract per-branch outputs from the fork node's output shape
/// (`{ outputs: [{ branch_id, output, success }] }`) or from a plain array.
fn collect_branch_outputs(input: &Value) -> Vec<Value> {
    if let Some(outputs) = input.get("outputs").and_then(|v| v.as_array()) {
        return outputs
            .iter()
            .filter(|entry| {
                entry
                    .get("success")
                    .and_then(|s| s.as_bool())
                    .unwrap_or(true)
            })
            .filter_map(|entry| entry.get("output").cloned())
            .collect();
    }
    if let Some(arr) = input.as_array() {
        return arr.clone();
    }
    Vec::new()
}

/// Merge strategy: object fields merged field-by-field (arrays concatenated,
/// scalars last-wins). Non-object outputs fall back to last-wins.
fn merge_outputs(outputs: &[Value]) -> Value {
    let mut merged: serde_json::Map<String, Value> = serde_json::Map::new();
    for output in outputs {
        match output {
            Value::Object(map) => {
                for (key, value) in map {
                    match (merged.get_mut(key), value) {
                        (Some(Value::Array(prev)), Value::Array(items)) => {
                            prev.extend(items.clone());
                        }
                        (Some(prev), value) => {
                            *prev = value.clone();
                        }
                        (None, value) => {
                            merged.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            value => {
                return value.clone();
            }
        }
    }
    Value::Object(merged)
}

#[async_trait]
impl NodeHandler for JoinHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Join
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let strategy = config
            .get("join_strategy")
            .and_then(|s| s.as_str())
            .unwrap_or("wait_for_all");

        let event_bus = ctx.event_bus.clone();
        let join_meta = HashMap::from([
            (
                "join_strategy".to_string(),
                Value::String(strategy.to_string()),
            ),
            ("node_id".to_string(), Value::String(ctx.node_id.clone())),
        ]);
        if let Some(bus) = &event_bus {
            let _ = bus.publish(BaseEvent {
                id: wf_types::Id::new(),
                r#type: EventType::WorkflowExecutionJoinStarted,
                timestamp: wf_common::now(),
                workflow_id: None,
                execution_id: Some(ctx.execution_id.clone()),
                agent_loop_id: None,
                metadata: Some(join_meta),
            });
        }

        let branch_outputs = collect_branch_outputs(&ctx.input);
        let aggregated = if branch_outputs.is_empty() {
            ctx.input.clone()
        } else {
            match strategy {
                // wait_for_any returns the first successful branch output.
                "wait_for_any" => branch_outputs[0].clone(),
                // wait_for_n merges the first `threshold` successful outputs.
                "wait_for_n" => {
                    let threshold = config
                        .get("threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(branch_outputs.len() as u64)
                        as usize;
                    let taken: Vec<Value> = branch_outputs
                        .iter()
                        .take(threshold.max(1))
                        .cloned()
                        .collect();
                    merge_outputs(&taken)
                }
                // wait_for_all merges every successful branch output.
                _ => merge_outputs(&branch_outputs),
            }
        };

        if let Some(bus) = &event_bus {
            let _ = bus.publish(BaseEvent {
                id: wf_types::Id::new(),
                r#type: EventType::WorkflowExecutionJoinCompleted,
                timestamp: wf_common::now(),
                workflow_id: None,
                execution_id: Some(ctx.execution_id.clone()),
                agent_loop_id: None,
                metadata: Some(HashMap::from([
                    (
                        "join_strategy".to_string(),
                        Value::String(strategy.to_string()),
                    ),
                    (
                        "branch_count".to_string(),
                        Value::Number(serde_json::Number::from(branch_outputs.len() as u64)),
                    ),
                ])),
            });
        }

        Ok(NodeExecutionResult::simple(aggregated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Branch "a1" contains a nested fork that converges at njoin.
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
        assert_eq!(
            ids,
            vec!["a1", "b1", "b2", "fork", "join", "nfork", "njoin"]
        );
        assert_eq!(subgraph.end_node_ids, vec!["join"]);

        // Branch "a2" is a straight line to join.
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
        assert_eq!(ids2, vec!["a2", "fork", "join", "njoin"]);
    }

    #[test]
    fn join_collects_fork_outputs() {
        let input = serde_json::json!({
            "results": [],
            "outputs": [
                {"branch_id": "b1", "output": {"x": 1}, "success": true},
                {"branch_id": "b2", "output": {"y": 2}, "success": true},
                {"branch_id": "b3", "output": {"x": 9}, "success": false}
            ]
        });
        let outputs = collect_branch_outputs(&input);
        assert_eq!(outputs.len(), 2);
        assert_eq!(merge_outputs(&outputs), serde_json::json!({"x": 1, "y": 2}));
    }

    #[test]
    fn join_merge_concats_arrays() {
        let outputs = vec![
            serde_json::json!({"items": [1], "n": 1}),
            serde_json::json!({"items": [2, 3], "n": 2}),
        ];
        assert_eq!(
            merge_outputs(&outputs),
            serde_json::json!({"items": [1, 2, 3], "n": 2})
        );
    }

    #[tokio::test]
    async fn join_strategies_aggregate_branch_outputs() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let input = serde_json::json!({
            "outputs": [
                {"branch_id": "b1", "output": {"x": 1}, "success": true},
                {"branch_id": "b2", "output": {"y": 2}, "success": true}
            ]
        });

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "join1".to_string(),
            StaticNodeType::Join,
            input,
            vars,
        )
        .with_node_config(serde_json::json!({"join_strategy": "wait_for_all"}));
        let result = JoinHandler
            .execute(&mut ctx)
            .await
            .expect("wait_for_all should merge all outputs");
        assert_eq!(result.output, serde_json::json!({"x": 1, "y": 2}));

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "join1".to_string(),
            StaticNodeType::Join,
            serde_json::json!({
                "outputs": [
                    {"branch_id": "b1", "output": {"x": 1}, "success": true},
                    {"branch_id": "b2", "output": {"y": 2}, "success": true}
                ]
            }),
            std::sync::Arc::new(dashmap::DashMap::new()),
        )
        .with_node_config(serde_json::json!({"join_strategy": "wait_for_any"}));
        let result = JoinHandler
            .execute(&mut ctx)
            .await
            .expect("wait_for_any should return the first output");
        assert_eq!(result.output, serde_json::json!({"x": 1}));

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "join1".to_string(),
            StaticNodeType::Join,
            serde_json::json!({
                "outputs": [
                    {"branch_id": "b1", "output": {"x": 1}, "success": true},
                    {"branch_id": "b2", "output": {"y": 2}, "success": true}
                ]
            }),
            std::sync::Arc::new(dashmap::DashMap::new()),
        )
        .with_node_config(serde_json::json!({
            "join_strategy": "wait_for_n",
            "threshold": 1
        }));
        let result = JoinHandler
            .execute(&mut ctx)
            .await
            .expect("wait_for_n should merge up to the threshold");
        assert_eq!(result.output, serde_json::json!({"x": 1}));
    }

    #[test]
    fn find_join_matches_by_path_ids() {
        let nodes = vec![
            WorkflowNode {
                id: "join1".into(),
                name: None,
                node_type: "JOIN".into(),
                inner: serde_json::json!({"fork_path_ids": ["p1", "p2"]}),
            },
            WorkflowNode {
                id: "join2".into(),
                name: None,
                node_type: "JOIN".into(),
                inner: serde_json::json!({"fork_path_ids": ["p3"]}),
            },
        ];
        let graph = build_graph(nodes, vec![]);
        assert_eq!(
            find_join_node(&graph, &["p1".to_string(), "p2".to_string()]),
            Some("join1".to_string())
        );
        assert_eq!(
            find_join_node(&graph, &["p3".to_string()]),
            Some("join2".to_string())
        );
        assert_eq!(find_join_node(&graph, &["p9".to_string()]), None);
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
        }
    }
}

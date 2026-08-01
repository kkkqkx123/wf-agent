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
) -> Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>> {
    match &ctx.handler_registry {
        Some(any) => match any
            .clone()
            .downcast::<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>()
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

pub struct ForkHandler;

#[async_trait]
impl NodeHandler for ForkHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Fork
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let branches = config
            .get("branches")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();

        if branches.is_empty() {
            return Err(WorkflowError::ForkJoinError(
                "No branches defined for fork node".to_string(),
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
                    Value::Number(serde_json::Number::from(branches.len() as u64)),
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
        let join_node_id = config
            .get("target_join")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        let tool_registry = ctx.tool_registry.clone();
        let parent_variables = ctx.variables.clone();

        let barrier = Arc::new(SyncBarrier::new(branches.len()));
        let mut handles = Vec::new();
        let event_bus_clone = event_bus.clone();
        let execution_id_clone = execution_id.clone();
        let node_id_clone = node_id.clone();

        for (idx, branch) in branches.iter().enumerate() {
            let branch_id = branch
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("branch")
                .to_string();
            let branch_input = branch.get("input").cloned().unwrap_or(ctx.input.clone());
            let barrier_clone = barrier.clone();
            let eb = event_bus_clone.clone();
            let eid = execution_id_clone.clone();
            let nid = node_id_clone.clone();
            let handlers = handlers.clone();
            let graph = graph.clone();
            let join_node_id = join_node_id.clone();
            let tool_registry = tool_registry.clone();
            let parent_variables = parent_variables.clone();

            emit_fork_event(
                event_bus.as_ref(),
                EventType::ForkBranchStarted,
                &execution_id,
                HashMap::from([
                    ("branch_id".to_string(), Value::String(branch_id.clone())),
                    (
                        "branch_index".to_string(),
                        Value::Number(serde_json::Number::from(idx as u64)),
                    ),
                    ("node_id".to_string(), Value::String(node_id.clone())),
                ]),
            );

            let handle = tokio::spawn(async move {
                let result = match &graph {
                    Some(g) => {
                        let outgoing: Vec<&WorkflowEdge> =
                            g.edges.iter().filter(|e| e.source_node_id == nid).collect();

                        let branch_edge = outgoing
                            .iter()
                            .find(|e| {
                                e.label.as_deref() == Some(&branch_id)
                                    || e.target_node_id == branch_id
                            })
                            .or_else(|| outgoing.get(idx))
                            .or_else(|| outgoing.first());

                        match branch_edge {
                            Some(edge) => {
                                let join_target = join_node_id.clone().unwrap_or_default();
                                let subgraph = extract_branch_subgraph(g, &nid, edge, &join_target);

                                if subgraph.nodes.is_empty() {
                                    BranchResult::success(&branch_id, branch_input)
                                } else {
                                    match execute_branch(
                                        &eid,
                                        &branch_id,
                                        branch_input,
                                        subgraph,
                                        BranchContext {
                                            handlers,
                                            event_bus: eb,
                                            tool_registry,
                                            parent_variables,
                                        },
                                    )
                                    .await
                                    {
                                        Ok(output) => output,
                                        Err(e) => BranchResult::failure(&branch_id, e.to_string()),
                                    }
                                }
                            }
                            None => BranchResult::success(&branch_id, branch_input),
                        }
                    }
                    None => BranchResult::success(&branch_id, branch_input),
                };
                barrier_clone.notify_branch_completed(&branch_id).await;
                result
            });
            handles.push(handle);
        }

        let results: Vec<BranchResult> = join_all(handles)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

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
                    Value::Number(serde_json::Number::from(branches.len() as u64)),
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
            Value::Number(serde_json::Number::from(branches.len() as u64)),
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
            let target = config.get("target_join").and_then(|t| t.as_str());
            if let Some(target) = target {
                next_nodes.push(target.to_string());
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
    handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    event_bus: Option<Arc<EventBus>>,
    tool_registry: Option<Arc<ToolRegistry>>,
    parent_variables: Arc<dashmap::DashMap<String, Value>>,
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

/// Numeric aggregation over branch outputs (each output must be numeric or a
/// single-number array/object is treated as scalar via `as_f64`).
fn numeric_aggregate(outputs: &[Value], handler: &str) -> Option<Value> {
    let numbers: Vec<f64> = outputs.iter().filter_map(|v| v.as_f64()).collect();
    if numbers.is_empty() {
        return None;
    }
    let result = match handler {
        "sum" => numbers.iter().sum::<f64>(),
        "average" | "avg" => numbers.iter().sum::<f64>() / numbers.len() as f64,
        "max" => numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        "min" => numbers.iter().cloned().fold(f64::INFINITY, f64::min),
        _ => return None,
    };
    serde_json::Number::from_f64(result).map(Value::Number)
}

/// Concatenation aggregation: joins all array outputs into one array.
fn concat_outputs(outputs: &[Value]) -> Value {
    let mut items: Vec<Value> = Vec::new();
    for output in outputs {
        match output {
            Value::Array(arr) => items.extend(arr.clone()),
            other => items.push(other.clone()),
        }
    }
    Value::Array(items)
}

/// Run a user-supplied JS aggregation function against the branch outputs.
/// `handler` is treated as an expression evaluating to a function that takes
/// the results array and returns the aggregated value (sync or async).
async fn run_custom_aggregator(handler: &str, outputs: &[Value]) -> WorkflowResult<Value> {
    let sandbox = std::sync::Arc::new(wf_sandbox::SandboxRuntime::new());
    let config = crate::handler::script::ScriptHandler::build_sandbox_config(None, "javascript");
    let results_json = serde_json::to_string(outputs).map_err(|e| {
        WorkflowError::ForkJoinError(format!("Failed to serialize branch outputs: {}", e))
    })?;
    let code = format!(
        "const __results = {};\n(async () => {{\n  const __out = ({})(__results);\n  const __final = __out && typeof __out.then === 'function' ? await __out : __out;\n  console.log(JSON.stringify(__final));\n}})();",
        results_json, handler
    );

    let result = sandbox.execute("javascript", &code, &config).await;
    if !result.success {
        return Err(WorkflowError::ForkJoinError(format!(
            "Custom join aggregation failed: {}",
            result.stderr.as_deref().unwrap_or("unknown error")
        )));
    }
    let stdout = result.stdout.unwrap_or_default();
    let parsed = serde_json::from_str::<Value>(&stdout).map_err(|_| {
        WorkflowError::ForkJoinError(format!(
            "Custom join aggregation returned non-JSON: {}",
            stdout
        ))
    })?;
    Ok(parsed)
}

#[async_trait]
impl NodeHandler for JoinHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Join
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let strategy = config
            .get("strategy")
            .and_then(|s| s.as_str())
            .unwrap_or("merge");

        let event_bus = ctx.event_bus.clone();
        let join_meta = HashMap::from([
            ("strategy".to_string(), Value::String(strategy.to_string())),
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
                "first" => branch_outputs[0].clone(),
                "last" => branch_outputs.last().unwrap().clone(),
                "merge" => merge_outputs(&branch_outputs),
                "aggregate" => {
                    let handler = config.get("handler").and_then(|h| h.as_str());
                    match handler {
                        Some(h) if matches!(h, "sum" | "average" | "avg" | "max" | "min") => {
                            numeric_aggregate(&branch_outputs, h)
                                .unwrap_or_else(|| merge_outputs(&branch_outputs))
                        }
                        Some("concat") => concat_outputs(&branch_outputs),
                        Some(h) => run_custom_aggregator(h, &branch_outputs).await?,
                        None => merge_outputs(&branch_outputs),
                    }
                }
                _ => ctx.input.clone(),
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
                    ("strategy".to_string(), Value::String(strategy.to_string())),
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

    #[test]
    fn join_numeric_aggregates() {
        let outputs = vec![
            serde_json::json!(1),
            serde_json::json!(2),
            serde_json::json!(3),
        ];
        assert_eq!(
            numeric_aggregate(&outputs, "sum"),
            Some(serde_json::json!(6.0))
        );
        assert_eq!(
            numeric_aggregate(&outputs, "average"),
            Some(serde_json::json!(2.0))
        );
        assert_eq!(
            numeric_aggregate(&outputs, "max"),
            Some(serde_json::json!(3.0))
        );
        assert_eq!(
            numeric_aggregate(&outputs, "min"),
            Some(serde_json::json!(1.0))
        );
        assert_eq!(concat_outputs(&outputs), serde_json::json!([1, 2, 3]));
    }

    #[tokio::test]
    async fn custom_aggregator_runs_user_function() {
        let outputs = vec![
            serde_json::json!({"v": 1}),
            serde_json::json!({"v": 2}),
            serde_json::json!({"v": 3}),
        ];
        let sum = run_custom_aggregator(
            "(results) => results.reduce((acc, r) => acc + r.v, 0)",
            &outputs,
        )
        .await
        .expect("sync aggregator should run");
        assert_eq!(sum.as_f64(), Some(6.0));

        let async_avg = run_custom_aggregator(
            "async (results) => results.reduce((acc, r) => acc + r.v, 0) / results.length",
            &outputs,
        )
        .await
        .expect("async aggregator should run");
        assert_eq!(async_avg.as_f64(), Some(2.0));
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

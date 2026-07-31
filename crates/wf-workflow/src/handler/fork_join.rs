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

fn extract_branch_subgraph(
    graph: &WorkflowGraphStructure,
    fork_node_id: &str,
    branch_edge: &WorkflowEdge,
    join_node_id: &str,
) -> WorkflowGraphStructure {
    let mut branch_nodes: HashSet<String> = HashSet::new();
    let mut branch_edges: Vec<WorkflowEdge> = Vec::new();

    let mut current = branch_edge.target_node_id.clone();
    let edge_map: HashMap<&str, Vec<&WorkflowEdge>> =
        graph.edges.iter().fold(HashMap::new(), |mut acc, e| {
            acc.entry(e.source_node_id.as_str())
                .or_insert_with(Vec::new)
                .push(e);
            acc
        });

    branch_nodes.insert(fork_node_id.to_string());
    while current != join_node_id {
        branch_nodes.insert(current.clone());
        if let Some(edges) = edge_map.get(current.as_str()) {
            for edge in edges {
                branch_edges.push((*edge).clone());
                branch_nodes.insert(edge.target_node_id.clone());
            }
            current = edges[0].target_node_id.clone();
        } else {
            break;
        }
    }

    let nodes: Vec<WorkflowNode> = graph
        .nodes
        .iter()
        .filter(|n| branch_nodes.contains(&n.id))
        .cloned()
        .collect();

    let last_node = branch_edges
        .last()
        .map(|e| e.target_node_id.clone())
        .unwrap_or_else(|| branch_edge.target_node_id.clone());

    WorkflowGraphStructure {
        nodes,
        edges: branch_edges,
        start_node_id: Some(branch_edge.target_node_id.clone()),
        end_node_ids: vec![last_node],
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
                                        handlers,
                                        eb,
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

async fn execute_branch(
    parent_execution_id: &wf_types::Id,
    branch_id: &str,
    input: Value,
    subgraph: WorkflowGraphStructure,
    handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
    event_bus: Option<Arc<EventBus>>,
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

    let tool_registry = Arc::new(ToolRegistry::new());
    let exec_ctx = ExecutorContext::new(
        execution_id.clone(),
        workflow_id.clone(),
        event_bus,
        tool_registry,
        options,
    )
    .with_parent_execution(parent_execution_id.clone());

    let entity = WorkflowExecutionEntity::new(execution_id.clone(), workflow_id);

    let mut coordinator: WorkflowCoordinator =
        WorkflowCoordinator::new(exec_ctx, subgraph, handlers)?.with_entity(entity);

    match coordinator.execute().await {
        Ok(output) => Ok(BranchResult::success(branch_id, output)),
        Err(e) => Ok(BranchResult::failure(branch_id, e.to_string())),
    }
}

pub struct JoinHandler;

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

        let aggregated = match strategy {
            "first" => ctx.input.clone(),
            "last" => ctx.input.clone(),
            "merge" => {
                if let Value::Object(map) = &ctx.input {
                    Value::Object(map.clone())
                } else {
                    ctx.input.clone()
                }
            }
            "aggregate" => {
                let handler = config.get("handler").and_then(|h| h.as_str());
                if handler == Some("sum") {
                    if let Value::Array(items) = &ctx.input {
                        let sum: f64 = items.iter().filter_map(|v| v.as_f64()).sum();
                        Value::Number(
                            serde_json::Number::from_f64(sum)
                                .unwrap_or(serde_json::Number::from(0)),
                        )
                    } else {
                        ctx.input.clone()
                    }
                } else {
                    ctx.input.clone()
                }
            }
            _ => ctx.input.clone(),
        };

        if let Some(bus) = &event_bus {
            let _ = bus.publish(BaseEvent {
                id: wf_types::Id::new(),
                r#type: EventType::WorkflowExecutionJoinCompleted,
                timestamp: wf_common::now(),
                workflow_id: None,
                execution_id: Some(ctx.execution_id.clone()),
                agent_loop_id: None,
                metadata: Some(HashMap::from([(
                    "strategy".to_string(),
                    Value::String(strategy.to_string()),
                )])),
            });
        }

        Ok(NodeExecutionResult::simple(aggregated))
    }
}

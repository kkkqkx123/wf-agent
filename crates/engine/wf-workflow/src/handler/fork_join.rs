use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wf_core::EventBus;
use wf_execution_shared::approval::ToolApprovalHandler;
use wf_execution_shared::context::NodeInputShape;
use wf_execution_shared::context::{ExecutorContext, NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::fork::{BranchStatus, ForkRegistry};
use wf_tools::registry::ToolRegistry;
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};

use crate::barrier::{BranchResult, FailureStrategy, ForkOutcome};
use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::resolve_handler_registry;
use crate::handler::NodeHandler;

fn emit_fork_event(
    event_bus: Option<&Arc<EventBus>>,
    event_type: EventType,
    execution_id: &wf_types::Id,
    metadata: HashMap<String, Value>,
) {
    let Some(bus) = event_bus else {
        tracing::debug!(execution_id = %execution_id, ?event_type, "no event bus, skipping fork/join event");
        return;
    };
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: event_type,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.clone()),
        agent_loop_id: None,

        event_name: None,
        metadata: Some(metadata),
    };
    let context = format!("workflow={} fork-join-event", execution_id);
    bus.publish_logged(event, &context).ok();
}

/// Extract the subgraph reachable from a fork branch edge, excluding the
/// join node. Uses BFS so nested forks, parallel sub-branches and converging
/// paths inside the branch are all collected; the join node belongs to the
/// parent graph only (branches end at the node before the join).
fn extract_branch_subgraph(
    graph: &WorkflowGraphStructure,
    _fork_node_id: &str,
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
    }
}

/// Find the JOIN node that the fork's branches converge to, structurally:
/// follow the graph edges from every branch edge of the fork and pick the
/// earliest JOIN-type node reachable from *all* branches. Nested forks stay
/// out of the result because their join is reachable from only one of the
/// outer fork's branches (the intersection of the per-branch reachable sets
/// is empty for them). Replaces the former `fork_path_ids` string matching
/// between FORK and JOIN configs, which broke across forks sharing path ids.
fn find_join_node(graph: &WorkflowGraphStructure, fork_node_id: &str) -> Option<String> {
    let branch_edges: Vec<&WorkflowEdge> = graph
        .edges
        .iter()
        .filter(|e| e.source_node_id == fork_node_id)
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
                .filter(|e| e.source_node_id == current)
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

pub struct ForkHandler;

#[async_trait]
impl NodeHandler for ForkHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Fork
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl ForkHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
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

        // Branch-level timeouts (`childExecutionTimeout`
        // and `totalBranchTimeout` fields). A positive `child_execution_timeout`
        // bounds each branch individually; a positive `total_branch_timeout`
        // bounds the whole fork.
        let child_execution_timeout = config
            .get("child_execution_timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total_branch_timeout = config
            .get("total_branch_timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

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

        let graph: Option<WorkflowGraphStructure> =
            ctx.graph_structure.as_ref().map(|g| (**g).clone());

        let handlers = resolve_handler_registry(ctx)?;
        let path_ids: Vec<String> = paths
            .iter()
            .filter_map(|p| p.get("path_id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        // The JOIN node is derived structurally: the earliest JOIN-type
        // node all branch edges converge to (no config string matching).
        let join_node_id = graph.as_ref().and_then(|g| find_join_node(g, &node_id));
        let tool_registry = ctx.tool_registry.clone();
        let parent_variables = ctx.variables.clone();
        let resource_registries = ctx.resource_registries.clone();
        let tool_approval_options = ctx.tool_approval_options.clone();
        let tool_approval_handler = ctx.tool_approval_handler.clone();
        // Retry budget policy: by default each branch consumes its own
        // allocated slice of the shared budget (`allocate_branch_budgets`,
        // equal split, pool borrowing only from the unallocated remainder) so
        // concurrent branches cannot starve each other. `share_retry_budget:
        // true` opts into the legacy shared semantics: all branches consume
        // the global pool directly and a branch exhausting the budget denies
        // retries everywhere else.
        let share_retry_budget = config
            .get("share_retry_budget")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let retry_budget = ctx.retry_budget.clone();
        if let Some(budget) = &retry_budget {
            if !share_retry_budget {
                budget.allocate_branch_budgets(&path_ids);
            }
        }

        // Whether the fork handler waits for every branch to settle before
        // returning (blocking, default). `false` launches the branches and
        // returns immediately; the JOIN node then waits via the fork
        // registry. In non-blocking mode `total_branch_timeout` no longer
        // applies at the fork (the handler has returned) — the JOIN timeout
        // bounds the wait instead.
        let wait_for_completion = config
            .get("wait_for_completion")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let registry = ctx.fork_registries.get(&node_id).cloned();
        // Pre-generate a branch execution id per path so the branch events
        // and the fork registry carry the branch's own identity.
        let branches: Vec<(Value, wf_types::Id)> = paths
            .iter()
            .map(|p| (p.clone(), wf_common::generate_id()))
            .collect();
        if let Some(registry) = &registry {
            for (path, execution_id) in &branches {
                let path_id = path
                    .get("path_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("path");
                registry.register(path_id, execution_id.clone());
            }
        }

        let event_bus_clone = event_bus.clone();
        let execution_id_clone = execution_id.clone();
        let node_id_clone = node_id.clone();
        let branch_input = ctx.input.clone();
        let cancellation = ctx.cancellation.clone();
        let fork_registries = ctx.fork_registries.clone();

        let results: Vec<BranchResult> = if fork_strategy == "serial" {
            let mut results = Vec::new();
            for (idx, (path, branch_execution_id)) in branches.iter().enumerate() {
                results.push(
                    run_branch(
                        idx,
                        path.clone(),
                        branch_execution_id.clone(),
                        registry.clone(),
                        fork_registries.clone(),
                        event_bus_clone.clone(),
                        execution_id_clone.clone(),
                        node_id_clone.clone(),
                        handlers.clone(),
                        graph.clone(),
                        join_node_id.clone(),
                        tool_registry.clone(),
                        parent_variables.clone(),
                        resource_registries.clone(),
                        branch_input.clone(),
                        cancellation.clone(),
                        retry_budget.clone(),
                        tool_approval_options.clone(),
                        tool_approval_handler.clone(),
                        child_execution_timeout,
                    )
                    .await,
                );
                if cancellation.as_ref().is_some_and(|t| t.is_cancelled()) {
                    break;
                }
            }
            results
        } else if !wait_for_completion {
            // Non-blocking fork: spawn every branch, keep the task handles
            // in the registry (so the parent can abort them on cancellation)
            // and return immediately. The JOIN node waits for the branches
            // through the same registry.
            for (idx, (path, branch_execution_id)) in branches.iter().enumerate() {
                let path = path.clone();
                let branch_execution_id = branch_execution_id.clone();
                let path_id = path
                    .get("path_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("path")
                    .to_string();
                let handle = tokio::spawn({
                    let registry = registry.clone();
                    let fork_registries = fork_registries.clone();
                    let event_bus_clone = event_bus_clone.clone();
                    let execution_id_clone = execution_id_clone.clone();
                    let node_id_clone = node_id_clone.clone();
                    let handlers = handlers.clone();
                    let graph = graph.clone();
                    let join_node_id = join_node_id.clone();
                    let tool_registry = tool_registry.clone();
                    let parent_variables = parent_variables.clone();
                    let resource_registries = resource_registries.clone();
                    let branch_input = branch_input.clone();
                    let cancellation = cancellation.clone();
                    let retry_budget = retry_budget.clone();
                    let tool_approval_options = tool_approval_options.clone();
                    let tool_approval_handler = tool_approval_handler.clone();
                    async move {
                        let _ = run_branch(
                            idx,
                            path.clone(),
                            branch_execution_id.clone(),
                            registry,
                            fork_registries,
                            event_bus_clone,
                            execution_id_clone,
                            node_id_clone,
                            handlers,
                            graph,
                            join_node_id,
                            tool_registry,
                            parent_variables,
                            resource_registries,
                            branch_input,
                            cancellation,
                            retry_budget,
                            tool_approval_options,
                            tool_approval_handler,
                            child_execution_timeout,
                        )
                        .await;
                    }
                });
                if let Some(registry) = &registry {
                    registry.register_handle(&path_id, handle);
                }
            }
            Vec::new()
        } else {
            let mut set = tokio::task::JoinSet::new();
            for (idx, (path, branch_execution_id)) in branches.iter().enumerate() {
                set.spawn(run_branch(
                    idx,
                    path.clone(),
                    branch_execution_id.clone(),
                    registry.clone(),
                    fork_registries.clone(),
                    event_bus_clone.clone(),
                    execution_id_clone.clone(),
                    node_id_clone.clone(),
                    handlers.clone(),
                    graph.clone(),
                    join_node_id.clone(),
                    tool_registry.clone(),
                    parent_variables.clone(),
                    resource_registries.clone(),
                    branch_input.clone(),
                    cancellation.clone(),
                    retry_budget.clone(),
                    tool_approval_options.clone(),
                    tool_approval_handler.clone(),
                    child_execution_timeout,
                ));
            }
            let mut results = Vec::with_capacity(paths.len());
            let total_deadline = (total_branch_timeout > 0).then(|| {
                std::time::Instant::now() + std::time::Duration::from_millis(total_branch_timeout)
            });
            loop {
                let joined = match total_deadline {
                    Some(deadline) => {
                        let sleep = tokio::time::sleep_until(deadline.into());
                        tokio::pin!(sleep);
                        tokio::select! {
                            joined = set.join_next(), if !set.is_empty() => joined,
                            _ = &mut sleep => {
                                // The whole fork exceeded `total_branch_timeout`;
                                // abort in-flight branches and settle.
                                tracing::warn!(
                                    "fork exceeded total_branch_timeout ({}ms), aborting branches",
                                    total_branch_timeout
                                );
                                set.abort_all();
                                break;
                            }
                        }
                    }
                    None => set.join_next().await,
                };
                let Some(joined) = joined else {
                    break;
                };
                if cancellation.as_ref().is_some_and(|t| t.is_cancelled()) {
                    // The parent was cancelled: abort the remaining in-flight
                    // branches instead of letting them run to completion.
                    set.abort_all();
                    break;
                }
                match joined {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        // A panicking branch must surface as a failure, not be
                        // silently dropped (JoinError carries no payload).
                        results.push(BranchResult::failure(
                            "branch",
                            format!("fork branch panicked: {e}"),
                        ));
                    }
                }
            }
            results
        };

        let outcome = failure_strategy.evaluate(&results);

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

        // Hand the branch results to the JOIN over a shared variable keyed by
        // the fork node id. The JOIN (in the parent coordinator) reads it to
        // aggregate branch outputs; branches run in child coordinators whose
        // node outputs never reach the parent graph.
        ctx.set_internal_variable(format!("__fork_outputs_{}", node_id), output.clone());

        Ok(NodeExecutionResult {
            output,
            next_node_ids: next_nodes,
            metadata,
        })
    }
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

struct BranchContext {
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    event_bus: Option<Arc<EventBus>>,
    tool_registry: Option<Arc<ToolRegistry>>,
    resource_registries: Option<Arc<wf_resource::registry::ResourceRegistries>>,
    parent_variables: Arc<dashmap::DashMap<String, Value>>,
    retry_budget: Option<Arc<wf_common::retry::RetryBudget>>,
    tool_approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    tool_approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
    /// All fork registries of the parent execution (keyed by fork node id),
    /// so nested forks and SYNC nodes inside the branch resolve their forks.
    fork_registries: Arc<std::collections::HashMap<String, Arc<ForkRegistry>>>,
    /// The registry of the fork that launched this branch (live-variable
    /// progress sink).
    fork_registry: Option<Arc<ForkRegistry>>,
}

/// Execute one fork branch: extract the branch subgraph from the edge
/// carrying the path label and run it up to the join node. Emits the branch
/// lifecycle events with the branch's own execution id and settles the
/// branch in the fork registry.
#[allow(clippy::too_many_arguments)]
async fn run_branch(
    idx: usize,
    path: Value,
    branch_execution_id: wf_types::Id,
    fork_registry: Option<Arc<ForkRegistry>>,
    fork_registries: Arc<std::collections::HashMap<String, Arc<ForkRegistry>>>,
    event_bus: Option<Arc<EventBus>>,
    execution_id: wf_types::Id,
    node_id: String,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    graph: Option<WorkflowGraphStructure>,
    join_node_id: Option<String>,
    tool_registry: Option<Arc<ToolRegistry>>,
    parent_variables: Arc<dashmap::DashMap<String, Value>>,
    resource_registries: Option<Arc<wf_resource::registry::ResourceRegistries>>,
    branch_input: Value,
    cancellation: Option<CancellationToken>,
    retry_budget: Option<Arc<wf_common::retry::RetryBudget>>,
    tool_approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    tool_approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
    child_execution_timeout: u64,
) -> BranchResult {
    let path_id = path
        .get("path_id")
        .and_then(|v| v.as_str())
        .unwrap_or("path")
        .to_string();

    emit_fork_event(
        event_bus.as_ref(),
        EventType::ForkBranchStarted,
        &branch_execution_id,
        HashMap::from([
            ("branch_id".to_string(), Value::String(path_id.clone())),
            (
                "branch_index".to_string(),
                Value::Number(serde_json::Number::from(idx as u64)),
            ),
            ("node_id".to_string(), Value::String(node_id.clone())),
            (
                "parent_execution_id".to_string(),
                Value::String(execution_id.to_string()),
            ),
        ]),
    );

    let run = run_branch_inner(
        idx,
        &path,
        branch_execution_id.clone(),
        execution_id,
        node_id,
        event_bus.clone(),
        handlers,
        graph,
        join_node_id,
        tool_registry,
        parent_variables,
        resource_registries,
        branch_input,
        cancellation,
        retry_budget,
        tool_approval_options,
        tool_approval_handler,
        fork_registries,
        fork_registry.clone(),
    );

    let result = if child_execution_timeout > 0 {
        // A slow branch fails fast instead of blocking the fork past the
        // per-branch budget.
        match tokio::time::timeout(
            std::time::Duration::from_millis(child_execution_timeout),
            run,
        )
        .await
        {
            Ok(result) => result,
            Err(_) => BranchResult::failure(
                &path_id,
                format!(
                    "fork branch '{}' exceeded child_execution_timeout ({}ms)",
                    path_id, child_execution_timeout
                ),
            ),
        }
    } else {
        run.await
    };

    emit_fork_event(
        event_bus.as_ref(),
        EventType::ForkBranchCompleted,
        &branch_execution_id,
        HashMap::from([
            ("branch_id".to_string(), Value::String(path_id.clone())),
            ("success".to_string(), Value::Bool(result.success)),
        ]),
    );

    // Record the settlement in the fork registry (idempotent; first
    // settlement wins). Wakes SYNC/JOIN waiters.
    if let Some(registry) = &fork_registry {
        registry.settle(
            &path_id,
            result.success,
            result.output.clone(),
            result.error.clone(),
            result.variables.clone(),
        );
    }
    result
}

/// Extract the subgraph for a branch edge and run it up to the join node.
#[allow(clippy::too_many_arguments)]
async fn run_branch_inner(
    idx: usize,
    path: &Value,
    branch_execution_id: wf_types::Id,
    parent_execution_id: wf_types::Id,
    node_id: String,
    event_bus: Option<Arc<EventBus>>,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    graph: Option<WorkflowGraphStructure>,
    join_node_id: Option<String>,
    tool_registry: Option<Arc<ToolRegistry>>,
    parent_variables: Arc<dashmap::DashMap<String, Value>>,
    resource_registries: Option<Arc<wf_resource::registry::ResourceRegistries>>,
    branch_input: Value,
    cancellation: Option<CancellationToken>,
    retry_budget: Option<Arc<wf_common::retry::RetryBudget>>,
    tool_approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    tool_approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
    fork_registries: Arc<std::collections::HashMap<String, Arc<ForkRegistry>>>,
    fork_registry: Option<Arc<ForkRegistry>>,
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
                            &branch_execution_id,
                            &parent_execution_id,
                            &path_id,
                            branch_input,
                            subgraph,
                            BranchContext {
                                handlers,
                                event_bus,
                                tool_registry,
                                resource_registries,
                                parent_variables,
                                retry_budget,
                                tool_approval_options,
                                tool_approval_handler,
                                fork_registries,
                                fork_registry,
                            },
                            cancellation,
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
    result
}

async fn execute_branch(
    branch_execution_id: &wf_types::Id,
    parent_execution_id: &wf_types::Id,
    branch_id: &str,
    input: Value,
    subgraph: WorkflowGraphStructure,
    branch_ctx: BranchContext,
    cancellation: Option<CancellationToken>,
) -> WorkflowResult<BranchResult> {
    let execution_id = branch_execution_id.clone();
    let workflow_id = wf_common::generate_id();

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
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    };

    let tool_registry = branch_ctx
        .tool_registry
        .unwrap_or_else(|| Arc::new(ToolRegistry::new()));
    let mut exec_ctx = ExecutorContext::new(
        execution_id.clone(),
        workflow_id.clone(),
        branch_ctx.event_bus,
        tool_registry,
        options,
    )
    .with_parent_execution(parent_execution_id.clone());
    if let Some(ref regs) = branch_ctx.resource_registries {
        exec_ctx = exec_ctx.with_resource_registries(regs.clone());
    }
    // Branches inherit the parent's tool-level approval config.
    if branch_ctx.tool_approval_options.is_some() || branch_ctx.tool_approval_handler.is_some() {
        exec_ctx = exec_ctx.with_tool_approval(
            branch_ctx.tool_approval_options.clone(),
            branch_ctx.tool_approval_handler.clone(),
        );
    }
    // Branches inherit the parent's fork registries so SYNC/JOIN nodes inside
    // the branch resolve their fork.
    if !branch_ctx.fork_registries.is_empty() {
        exec_ctx = exec_ctx.with_fork_registries(branch_ctx.fork_registries.clone());
    }
    // Inherit retry budget from parent execution.
    if let Some(budget) = branch_ctx.retry_budget.as_ref() {
        exec_ctx = exec_ctx.with_retry_budget(budget.clone());
    }
    // Branches inherit a read-only snapshot of the parent variables.
    crate::handler::variable_mapping::inherit_all_variables(
        &branch_ctx.parent_variables,
        &exec_ctx.variables,
    );

    let branch_variables = exec_ctx.variables.clone();

    let mut entity = WorkflowExecutionEntity::new(execution_id.clone(), workflow_id);
    // Inherit retry budget from parent execution.
    if let Some(budget) = branch_ctx.retry_budget.as_ref() {
        entity = entity.with_retry_budget(budget.clone());
    }

    let mut coordinator: WorkflowCoordinator =
        WorkflowCoordinator::new(exec_ctx, subgraph, branch_ctx.handlers)?.with_entity(entity);
    // Publish the branch's public variables after every completed node so
    // SYNC nodes can read the source branch's intermediate state.
    if let Some(registry) = &branch_ctx.fork_registry {
        coordinator =
            coordinator.with_fork_branch_progress(registry.clone(), branch_id.to_string());
    }

    let run = coordinator.execute();
    let run_result = match cancellation {
        Some(token) => tokio::select! {
            result = run => result,
            _ = token.cancelled() => {
                Err(WorkflowError::CoordinatorError(
                    "fork branch cancelled by parent".to_string(),
                ))
            }
        },
        None => run.await,
    };

    match run_result {
        Ok(output) => Ok(BranchResult::success_with_variables(
            branch_id,
            output,
            public_variables(&branch_variables),
        )),
        Err(e) => Ok(BranchResult::failure(branch_id, e.to_string())),
    }
}

/// Snapshot the branch's public variables (excluding `__`-prefixed internal
/// state such as loop stacks, message contexts and fork handovers).
fn public_variables(
    variables: &Arc<dashmap::DashMap<String, Value>>,
) -> std::collections::HashMap<String, Value> {
    variables
        .iter()
        .filter(|entry| !entry.key().starts_with("__"))
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect()
}

pub struct JoinHandler;

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

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl JoinHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config: Value = ctx.node_config.clone().unwrap_or(Value::Null);
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
        match &event_bus {
            Some(bus) => {
                bus.publish_logged(
                    BaseEvent {
                        id: wf_types::Id::new(),
                        r#type: EventType::WorkflowExecutionJoinStarted,
                        timestamp: wf_common::now(),
                        workflow_id: None,
                        execution_id: Some(ctx.execution_id.clone()),
                        agent_loop_id: None,

                        event_name: None,
                        metadata: Some(join_meta),
                    },
                    &format!("workflow={} join={}", ctx.execution_id, ctx.node_id),
                )
                .ok();
            }
            None => {
                tracing::debug!(
                    execution_id = %ctx.execution_id,
                    node_id = %ctx.node_id,
                    "no event bus, skipping join event"
                );
            }
        }

        // Prefer the fork registry (live branch records keyed by path id),
        // falling back to the recorded `__fork_outputs_<fork_id>` variable
        // (one-version compatibility) and then to the local input when the
        // JOIN was reached without a fork (e.g. resumed from a checkpoint).
        // The local input is only interpretable as a fork output shape when
        // it arrived as a merged multi-edge object (`Merged`); a bare single
        // value is treated as a pass-through payload.
        let fork_id = ctx
            .graph_structure
            .as_ref()
            .and_then(|g| find_fork_node(ctx, g));
        let registry = fork_id
            .as_ref()
            .and_then(|id| ctx.fork_registries.get(id))
            .cloned();
        let mut path_ids: Vec<String> = config
            .get("fork_path_ids")
            .and_then(|v| v.as_array())
            .map(|ids| {
                ids.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        if path_ids.is_empty() {
            if let Some(registry) = &registry {
                path_ids = registry.path_ids();
            }
        }

        let raw_results = if let Some(registry) = &registry {
            // The JOIN wait semantics: wait until the strategy's required
            // number of branches settle (bounded by the JOIN `timeout`; a
            // non-blocking fork returns before the branches finish). On
            // timeout the JOIN fails.
            let join_timeout = config.get("timeout").and_then(|v| v.as_u64());
            let required = match strategy {
                "wait_for_any" | "WaitForAny" => 1,
                "wait_for_n" | "WaitForN" => config
                    .get("threshold")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as usize,
                _ => path_ids.len(),
            };
            let wait = registry.wait_for_count(&path_ids, required, join_timeout);
            let ok = match &ctx.cancellation {
                Some(token) => tokio::select! {
                    ok = wait => ok,
                    _ = token.cancelled() => {
                        registry.abort_all();
                        false
                    }
                },
                None => wait.await,
            };
            if !ok {
                return Err(WorkflowError::ForkJoinError(format!(
                    "JOIN node '{}' timed out waiting for fork branches to settle",
                    ctx.node_id
                )));
            }
            registry
                .records(&path_ids)
                .into_iter()
                .map(|(path_id, record)| match record.status {
                    BranchStatus::Completed => BranchResult::success_with_variables(
                        path_id,
                        record.output.clone().unwrap_or(Value::Null),
                        record.variables.clone(),
                    ),
                    BranchStatus::Failed => BranchResult::failure(
                        path_id,
                        record
                            .error
                            .clone()
                            .unwrap_or_else(|| "branch failed".to_string()),
                    ),
                    BranchStatus::Cancelled => BranchResult::failure(path_id, "branch cancelled"),
                    BranchStatus::Running => BranchResult::failure(path_id, "branch still running"),
                })
                .collect()
        } else {
            let fork_output = fork_id
                .as_ref()
                .and_then(|id| ctx.get_variable(&format!("__fork_outputs_{}", id)));
            if let Some(output) = &fork_output {
                collect_branch_records(output)
            } else if ctx.input_shape == NodeInputShape::Merged {
                collect_branch_records(&ctx.input)
            } else {
                Vec::new()
            }
        };

        let success_records: Vec<BranchResult> =
            raw_results.iter().filter(|r| r.success).cloned().collect();
        let failed_records: Vec<BranchResult> =
            raw_results.iter().filter(|r| !r.success).cloned().collect();

        // Branch-level timeout: a positive `timeout` bounds the JOIN's wait for
        // the branches (relevant for non-blocking forks); joined-late or
        // aborted branches are treated as failures.
        let expected = path_ids.len().max(raw_results.len());

        let aggregated = if raw_results.is_empty() {
            ctx.input.clone()
        } else {
            match strategy {
                // wait_for_any returns the first successful branch output.
                "wait_for_any" | "WaitForAny" => success_records
                    .first()
                    .map(|r| r.output.clone())
                    .unwrap_or(ctx.input.clone()),
                // wait_for_n merges the first `threshold` successful outputs.
                "wait_for_n" | "WaitForN" => {
                    let threshold = config
                        .get("threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(success_records.len() as u64)
                        as usize;
                    let taken: Vec<Value> = success_records
                        .iter()
                        .take(threshold.max(1))
                        .map(|r| r.output.clone())
                        .collect();
                    merge_outputs(&taken)
                }
                // wait_for_all merges every successful branch output.
                _ => merge_outputs(
                    &success_records
                        .iter()
                        .map(|r| r.output.clone())
                        .collect::<Vec<_>>(),
                ),
            }
        };

        // JOIN aggregation: merge variables (by mapping), message contexts and
        // data outputs from the successful branches into the parent scope.
        aggregate_branch_variables(&config, &success_records, ctx)?;
        aggregate_branch_messages(ctx, &success_records);
        let mut aggregated = aggregated;
        aggregate_branch_data_outputs(&config, &success_records, &mut aggregated);

        match &event_bus {
            Some(bus) => {
                bus.publish_logged(
                    BaseEvent {
                        id: wf_types::Id::new(),
                        r#type: EventType::WorkflowExecutionJoinCompleted,
                        timestamp: wf_common::now(),
                        workflow_id: None,
                        execution_id: Some(ctx.execution_id.clone()),
                        agent_loop_id: None,

                        event_name: None,
                        metadata: Some(HashMap::from([
                            (
                                "join_strategy".to_string(),
                                Value::String(strategy.to_string()),
                            ),
                            (
                                "branch_count".to_string(),
                                Value::Number(serde_json::Number::from(
                                    success_records.len() as u64
                                )),
                            ),
                            (
                                "failed_branch_count".to_string(),
                                Value::Number(
                                    serde_json::Number::from(failed_records.len() as u64),
                                ),
                            ),
                            (
                                "expected_branches".to_string(),
                                Value::Number(serde_json::Number::from(expected as u64)),
                            ),
                        ])),
                    },
                    &format!("workflow={} join={}", ctx.execution_id, ctx.node_id),
                )
                .ok();
            }
            None => {
                tracing::debug!(
                    execution_id = %ctx.execution_id,
                    node_id = %ctx.node_id,
                    "no event bus, skipping join event"
                );
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "failed_branches".to_string(),
            Value::Array(
                failed_records
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "branch_id": r.branch_id,
                            "error": r.error.clone().unwrap_or_default(),
                        })
                    })
                    .collect(),
            ),
        );

        Ok(NodeExecutionResult {
            output: aggregated,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

/// Collect raw `BranchResult` records from a fork output shape
/// (`{ results: [...] }` or legacy `{ outputs: [...] }`) or a plain array.
fn collect_branch_records(input: &Value) -> Vec<BranchResult> {
    if let Some(results) = input.get("results").and_then(|v| v.as_array()) {
        return results
            .iter()
            .filter_map(|entry| serde_json::from_value::<BranchResult>(entry.clone()).ok())
            .collect();
    }
    // Legacy `outputs` shape: `[{ branch_id, output, success }]`.
    if let Some(outputs) = input.get("outputs").and_then(|v| v.as_array()) {
        return outputs
            .iter()
            .map(|entry| {
                let success = entry
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if success {
                    BranchResult::success(
                        entry
                            .get("branch_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("branch"),
                        entry.get("output").cloned().unwrap_or(Value::Null),
                    )
                } else {
                    BranchResult::failure(
                        entry
                            .get("branch_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("branch"),
                        entry
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("branch failed"),
                    )
                }
            })
            .collect();
    }
    if let Some(arr) = input.as_array() {
        return arr
            .iter()
            .filter_map(|entry| serde_json::from_value::<BranchResult>(entry.clone()).ok())
            .collect();
    }
    Vec::new()
}

/// Merge branch variables into the parent scope. A `variable_outputs`
/// mapping (`{ internal_name, target_path }`) copies each branch's exported
/// variable into the parent at its target path (last-writer-wins across
/// branches); without a mapping, branch variables are not imported
/// implicitly (explicit export only).
fn aggregate_branch_variables(
    config: &Value,
    success_records: &[BranchResult],
    ctx: &mut NodeExecutionContext,
) -> WorkflowResult<()> {
    let Some(mappings) = config.get("variable_outputs").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    for record in success_records {
        let Some(ref variables) = record.variables else {
            continue;
        };
        for mapping in mappings {
            let internal_name = mapping
                .get("internal_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let target_path = mapping
                .get("target_path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if internal_name.is_empty() || target_path.is_empty() {
                continue;
            }
            if let Some(value) = variables.get(internal_name) {
                crate::handler::variable_mapping::set_variable_path(
                    &ctx.variables,
                    target_path,
                    value.clone(),
                )?;
            }
        }
    }
    Ok(())
}

/// Merge message contexts from the successful branches into the parent
/// scope. A `message_outputs` mapping (`{ context_id, target_context_id }`)
/// copies each branch's named message array into the parent context.
fn aggregate_branch_messages(ctx: &mut NodeExecutionContext, success_records: &[BranchResult]) {
    let Some(config) = &ctx.node_config else {
        return;
    };
    let Some(outputs) = config.get("message_outputs").and_then(|v| v.as_array()) else {
        return;
    };
    for record in success_records {
        let Some(ref variables) = record.variables else {
            continue;
        };
        for mapping in outputs {
            let context_id = mapping
                .get("context_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let target_context_id = mapping
                .get("target_context_id")
                .and_then(|v| v.as_str())
                .unwrap_or(context_id);
            if context_id.is_empty() || target_context_id.is_empty() {
                continue;
            }
            let key = format!("{}{}", crate::message_context::CONTEXT_PREFIX, context_id);
            if let Some(value) = variables.get(&key) {
                if let Ok(messages) =
                    serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone())
                {
                    crate::message_context::append_context(
                        &ctx.variables,
                        target_context_id,
                        messages,
                    );
                }
            }
        }
    }
}

/// Merge `data_outputs` (`{ internal_name, output_key }`) from the
/// successful branches into the JOIN output: each mapping copies the named
/// branch variable into the aggregated output under `output_key`. The
/// primary record is the first successful branch (the main path); missing
/// values are skipped, so the output keeps whatever the strategy produced.
fn aggregate_branch_data_outputs(
    config: &Value,
    success_records: &[BranchResult],
    aggregated: &mut Value,
) {
    let Some(mappings) = config
        .get("data_outputs")
        .or_else(|| config.get("dataOutputs"))
        .and_then(|v| v.as_array())
    else {
        return;
    };
    let Some(primary) = success_records.first() else {
        return;
    };
    let Some(ref variables) = primary.variables else {
        return;
    };

    for mapping in mappings {
        let internal_name = mapping
            .get("internal_name")
            .or_else(|| mapping.get("internalName"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let output_key = mapping
            .get("output_key")
            .or_else(|| mapping.get("outputKey"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if internal_name.is_empty() || output_key.is_empty() {
            continue;
        }
        let Some(value) = variables.get(internal_name) else {
            continue;
        };
        if let Value::Object(map) = aggregated {
            map.insert(output_key.to_string(), value.clone());
        } else {
            let mut map = serde_json::Map::new();
            map.insert(output_key.to_string(), value.clone());
            *aggregated = Value::Object(map);
        }
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

        // Branch "a1" contains a nested fork that converges at njoin; the
        // outer join is deliberately excluded from the branch subgraph (it
        // belongs to the parent graph only).
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

        // Branch "a2" is a straight line to join (the join itself excluded).
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
    fn join_collects_fork_outputs() {
        let input = serde_json::json!({
            "results": [
                {"branch_id": "b1", "output": {"x": 1}, "success": true},
                {"branch_id": "b2", "output": {"y": 2}, "success": true},
                {"branch_id": "b3", "output": {"x": 9}, "success": false, "error": "boom"}
            ]
        });
        let records = collect_branch_records(&input);
        let outputs: Vec<Value> = records
            .iter()
            .filter(|r| r.success)
            .map(|r| r.output.clone())
            .collect();
        assert_eq!(outputs.len(), 2);
        assert_eq!(merge_outputs(&outputs), serde_json::json!({"x": 1, "y": 2}));
        // Failure branch info is preserved.
        assert_eq!(records[2].error.as_deref(), Some("boom"));
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
        ctx.input_shape = NodeInputShape::Merged;
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
        ctx.input_shape = NodeInputShape::Merged;
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
        ctx.input_shape = NodeInputShape::Merged;
        let result = JoinHandler
            .execute(&mut ctx)
            .await
            .expect("wait_for_n should merge up to the threshold");
        assert_eq!(result.output, serde_json::json!({"x": 1}));
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

        // Branches that never converge: no join.
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

        // A fork with no branch edges has nothing to join.
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
        // outer fork branches: (1) -> oa -> inner fork -> ia -> inner join
        //                    (2) -> ob -> ob2
        // both converge at outer_join. The inner join is reachable only from
        // branch (1), so the structural search must pick the outer join.
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

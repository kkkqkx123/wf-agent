use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wf_core::EventBus;
use wf_execution_shared::approval::ToolApprovalHandler;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::fork::ForkRegistry;
use wf_tools::registry::ToolRegistry;
use wf_types::events::EventType;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure,
};

use super::events::emit_fork_event;
use super::graph::extract_branch_subgraph;
use crate::barrier::BranchResult;
use crate::coordinator::WorkflowCoordinator;
use crate::entity::WorkflowExecutionEntity;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct BranchContext {
    pub handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    pub event_bus: Option<Arc<EventBus>>,
    pub tool_registry: Option<Arc<ToolRegistry>>,
    pub resource_registries: Option<Arc<wf_resource::registry::ResourceRegistries>>,
    pub parent_variables: Arc<dashmap::DashMap<String, Value>>,
    pub tool_approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    pub tool_approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
    /// All fork registries of the parent execution (keyed by fork node id),
    /// so nested forks and SYNC nodes inside the branch resolve their forks.
    pub fork_registries: Arc<std::collections::HashMap<String, Arc<ForkRegistry>>>,
    /// The registry of the fork that launched this branch (live-variable
    /// progress sink).
    pub fork_registry: Option<Arc<ForkRegistry>>,
}

/// Per-branch execution inputs bundled together so the branch runners take
/// only the branch-specific values; `branch_ctx` carries the runtime
/// environment shared by every branch of the fork.
pub struct BranchRunContext {
    pub parent_execution_id: wf_types::Id,
    pub parent_manager: Option<Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>>,
    pub parent_ancestors: Vec<wf_types::Id>,
    pub parent_depth: u32,
    pub parent_root: Option<wf_types::Id>,
    pub parent_checkpoints_enabled: Option<bool>,
    pub node_id: String,
    pub graph: Option<WorkflowGraphStructure>,
    pub join_node_id: Option<String>,
    pub branch_input: Value,
    pub cancellation: Option<CancellationToken>,
    pub child_execution_timeout: u64,
    pub branch_ctx: BranchContext,
}

/// Execute one fork branch: extract the branch subgraph from the edge
/// carrying the path label and run it up to the join node. Emits the branch
/// lifecycle events with the branch's own execution id and settles the
/// branch in the fork registry.
pub async fn run_branch(
    idx: usize,
    path: Value,
    branch_execution_id: wf_types::Id,
    ctx: BranchRunContext,
) -> BranchResult {
    let path_id = path
        .get("path_id")
        .and_then(|v| v.as_str())
        .unwrap_or("path")
        .to_string();

    emit_fork_event(
        ctx.branch_ctx.event_bus.as_ref(),
        EventType::ForkBranchStarted,
        &branch_execution_id,
        HashMap::from([
            ("branch_id".to_string(), Value::String(path_id.clone())),
            (
                "branch_index".to_string(),
                Value::Number(serde_json::Number::from(idx as u64)),
            ),
            ("node_id".to_string(), Value::String(ctx.node_id.clone())),
            (
                "parent_execution_id".to_string(),
                Value::String(ctx.parent_execution_id.to_string()),
            ),
        ]),
    );

    let event_bus = ctx.branch_ctx.event_bus.clone();
    let child_execution_timeout = ctx.child_execution_timeout;
    let fork_registry = ctx.branch_ctx.fork_registry.clone();
    let run = run_branch_inner(idx, &path, branch_execution_id.clone(), ctx);

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
            )
            .with_category(wf_types::workflow::error_branch::NodeErrorCategory::TransportTimeout),
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
            result.error_message().map(|s| s.to_string()),
            result.variables.clone(),
        );
    }
    result
}

/// Extract the subgraph for a branch edge and run it up to the join node.
async fn run_branch_inner(
    idx: usize,
    path: &Value,
    branch_execution_id: wf_types::Id,
    ctx: BranchRunContext,
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

    let result = match &ctx.graph {
        Some(g) => {
            let outgoing: Vec<&WorkflowEdge> = g
                .edges
                .iter()
                .filter(|e| {
                    e.source_node_id == ctx.node_id && !crate::error_branch::is_error_edge(e)
                })
                .collect();

            let branch_edge = outgoing
                .iter()
                .find(|e| e.label.as_deref() == Some(&path_id) || e.target_node_id == child_node_id)
                .or_else(|| outgoing.get(idx))
                .or_else(|| outgoing.first());

            match branch_edge {
                Some(edge) => {
                    let join_target = ctx.join_node_id.clone().unwrap_or_default();
                    let subgraph = extract_branch_subgraph(g, &ctx.node_id, edge, &join_target);

                    if subgraph.nodes.is_empty() {
                        BranchResult::success(&path_id, ctx.branch_input)
                    } else {
                        match execute_branch(
                            BranchHierarchy {
                                execution_id: branch_execution_id.clone(),
                                parent_execution_id: ctx.parent_execution_id.clone(),
                                parent_manager: ctx.parent_manager.clone(),
                                parent_ancestors: ctx.parent_ancestors.clone(),
                                parent_depth: ctx.parent_depth,
                                parent_root: ctx.parent_root.clone(),
                                parent_checkpoints_enabled: ctx.parent_checkpoints_enabled,
                                fork_node_id: ctx.node_id.clone(),
                            },
                            &path_id,
                            ctx.branch_input,
                            subgraph,
                            ctx.branch_ctx,
                            ctx.cancellation,
                        )
                        .await
                        {
                            Ok(output) => output,
                            Err(e) => BranchResult::failure(&path_id, e.to_string())
                                .with_category(crate::error_branch::classify_error(&e)),
                        }
                    }
                }
                None => BranchResult::success(&path_id, ctx.branch_input),
            }
        }
        None => BranchResult::success(&path_id, ctx.branch_input),
    };
    result
}

/// Execution-hierarchy context inherited by a fork branch from its parent.
struct BranchHierarchy {
    execution_id: wf_types::Id,
    parent_execution_id: wf_types::Id,
    parent_manager: Option<Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>>,
    parent_ancestors: Vec<wf_types::Id>,
    parent_depth: u32,
    parent_root: Option<wf_types::Id>,
    parent_checkpoints_enabled: Option<bool>,
    fork_node_id: String,
}

async fn execute_branch(
    hierarchy: BranchHierarchy,
    branch_id: &str,
    input: Value,
    subgraph: WorkflowGraphStructure,
    branch_ctx: BranchContext,
    cancellation: Option<CancellationToken>,
) -> WorkflowResult<BranchResult> {
    let BranchHierarchy {
        execution_id,
        parent_execution_id,
        parent_manager,
        parent_ancestors,
        parent_depth,
        parent_root,
        parent_checkpoints_enabled,
        fork_node_id,
    } = hierarchy;
    let workflow_id = wf_common::generate_id();

    let options = WorkflowExecutionOptions {
        input: Some(input),
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: parent_checkpoints_enabled,
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    };

    let tool_registry = branch_ctx
        .tool_registry
        .unwrap_or_else(|| Arc::new(ToolRegistry::new()));
    let fork_path = wf_types::execution::ForkPath::new(fork_node_id.clone(), branch_id.to_string());
    let child_manager = parent_manager.as_ref().and_then(|parent| {
        parent
            .derive_child(
                execution_id.clone(),
                wf_types::execution::ExecutionType::Workflow,
                Some(fork_path.clone()),
            )
            .ok()
    });
    let mut child_ancestors = parent_ancestors.to_vec();
    if child_ancestors.last() != Some(&parent_execution_id) {
        child_ancestors.push(parent_execution_id.clone());
    }
    let child_root = parent_root.unwrap_or_else(|| parent_execution_id.clone());
    let (child_ancestors, child_depth, child_root) = match child_manager.as_ref() {
        Some(manager) => (
            manager.ancestors(),
            manager.depth(),
            manager.root_execution_id(),
        ),
        None => (child_ancestors, parent_depth + 1, child_root),
    };
    let mut exec_ctx = ExecutorContext::new(
        execution_id.clone(),
        workflow_id.clone(),
        branch_ctx.event_bus,
        tool_registry,
        options,
    )
    .with_parent_execution(parent_execution_id.clone())
    .with_hierarchy(
        child_ancestors.clone(),
        child_depth,
        Some(child_root.clone()),
    );
    if let Some(manager) = child_manager.clone() {
        exec_ctx = exec_ctx.with_hierarchy_manager(manager);
    }
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
    // Branches inherit a read-only snapshot of the parent variables.
    crate::handler::variable_mapping::inherit_all_variables(
        &branch_ctx.parent_variables,
        &exec_ctx.variables,
    );

    let branch_variables = exec_ctx.variables.clone();

    let mut entity = WorkflowExecutionEntity::new(execution_id.clone(), workflow_id);
    if let Some(manager) = child_manager {
        entity = entity.with_hierarchy_manager(manager);
    } else {
        entity = entity
            .with_parent_execution_id(parent_execution_id.clone())
            .with_ancestors(child_ancestors)
            .with_hierarchy_depth(child_depth)
            .with_root_execution_id(child_root.clone());
    }
    entity =
        entity.with_execution_type(wf_types::workflow_execution::WorkflowExecutionType::ForkJoin);

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
                // Parent cancellation keeps its interruption category so the
                // settled branch result is not mistaken for a business failure.
                Err(WorkflowError::NodeFailure {
                    node_id: branch_id.to_string(),
                    category: wf_types::workflow::error_branch::NodeErrorCategory::CancelledInterrupted,
                    detail: "fork branch cancelled by parent".to_string(),
                    failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
                })
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
        Err(e) => Ok(BranchResult::failure(branch_id, e.to_string())
            .with_category(crate::error_branch::classify_error(&e))),
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

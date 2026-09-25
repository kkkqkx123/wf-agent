use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::events::EventType;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::WorkflowGraphStructure;

use super::branch::{run_branch, BranchContext, BranchRunContext};
use super::events::emit_fork_event;
use super::graph::find_join_node;
use crate::barrier::{BranchResult, FailureStrategy, ForkOutcome};
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::resolve_handler_registry;
use crate::handler::NodeHandler;

struct ForkConfig {
    paths: Vec<Value>,
    failure_strategy: FailureStrategy,
    fork_strategy: String,
    child_execution_timeout: u64,
    total_branch_timeout: u64,
    wait_for_completion: bool,
}

fn parse_fork_config(ctx: &NodeExecutionContext) -> WorkflowResult<ForkConfig> {
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
        .unwrap_or("parallel")
        .to_string();

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

    Ok(ForkConfig {
        paths,
        failure_strategy,
        fork_strategy,
        child_execution_timeout,
        total_branch_timeout,
        wait_for_completion,
    })
}

struct ForkRuntime {
    graph: Option<WorkflowGraphStructure>,
    join_node_id: Option<String>,
    event_bus: Option<Arc<EventBus>>,
    branch_input: Value,
    cancellation: Option<CancellationToken>,
    fork_registry: Option<Arc<wf_execution_shared::fork::ForkRegistry>>,
    branch_ctx: BranchContext,
    execution_id: wf_types::Id,
    node_id: String,
}

impl ForkRuntime {
    fn branch_run_context(&self, child_execution_timeout: u64) -> BranchRunContext {
        BranchRunContext {
            parent_execution_id: self.execution_id.clone(),
            node_id: self.node_id.clone(),
            graph: self.graph.clone(),
            join_node_id: self.join_node_id.clone(),
            branch_input: self.branch_input.clone(),
            cancellation: self.cancellation.clone(),
            child_execution_timeout,
            branch_ctx: BranchContext {
                handlers: self.branch_ctx.handlers.clone(),
                event_bus: self.branch_ctx.event_bus.clone(),
                tool_registry: self.branch_ctx.tool_registry.clone(),
                resource_registries: self.branch_ctx.resource_registries.clone(),
                parent_variables: self.branch_ctx.parent_variables.clone(),
                tool_approval_options: self.branch_ctx.tool_approval_options.clone(),
                tool_approval_handler: self.branch_ctx.tool_approval_handler.clone(),
                fork_registries: self.branch_ctx.fork_registries.clone(),
                fork_registry: self.branch_ctx.fork_registry.clone(),
            },
        }
    }
}

async fn run_serial(
    branches: &[(Value, wf_types::Id)],
    runtime: &ForkRuntime,
    child_execution_timeout: u64,
) -> Vec<BranchResult> {
    let mut results = Vec::new();
    for (idx, (path, branch_execution_id)) in branches.iter().enumerate() {
        results.push(
            run_branch(
                idx,
                path.clone(),
                branch_execution_id.clone(),
                runtime.branch_run_context(child_execution_timeout),
            )
            .await,
        );
        if runtime
            .cancellation
            .as_ref()
            .is_some_and(|t| t.is_cancelled())
        {
            break;
        }
    }
    results
}

fn spawn_non_blocking(
    branches: &[(Value, wf_types::Id)],
    runtime: &ForkRuntime,
    child_execution_timeout: u64,
) {
    for (idx, (path, branch_execution_id)) in branches.iter().enumerate() {
        let path = path.clone();
        let branch_execution_id = branch_execution_id.clone();
        let path_id = path
            .get("path_id")
            .and_then(|v| v.as_str())
            .unwrap_or("path")
            .to_string();
        let run_ctx = runtime.branch_run_context(child_execution_timeout);
        let log_id = path_id.clone();
        let handle = tokio::spawn(async move {
            let result = run_branch(idx, path, branch_execution_id, run_ctx).await;
            if !result.success {
                tracing::warn!(
                    branch = %log_id,
                    error = ?result.error,
                    "fire-and-forget fork branch ended with failure"
                );
            }
        });
        if let Some(registry) = &runtime.fork_registry {
            registry.register_handle(&path_id, handle);
        }
    }
}

async fn run_parallel(
    branches: &[(Value, wf_types::Id)],
    runtime: &ForkRuntime,
    child_execution_timeout: u64,
    total_branch_timeout: u64,
) -> Vec<BranchResult> {
    let mut set = tokio::task::JoinSet::new();
    for (idx, (path, branch_execution_id)) in branches.iter().enumerate() {
        set.spawn(run_branch(
            idx,
            path.clone(),
            branch_execution_id.clone(),
            runtime.branch_run_context(child_execution_timeout),
        ));
    }
    let mut results = Vec::with_capacity(branches.len());
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
        if runtime
            .cancellation
            .as_ref()
            .is_some_and(|t| t.is_cancelled())
        {
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
        let cfg = parse_fork_config(ctx)?;

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
                    Value::Number(serde_json::Number::from(cfg.paths.len() as u64)),
                ),
                ("node_id".to_string(), Value::String(node_id.clone())),
            ]),
        );

        let graph: Option<WorkflowGraphStructure> =
            ctx.graph_structure.as_ref().map(|g| (**g).clone());

        let handlers = resolve_handler_registry(ctx)?;
        // The JOIN node is derived structurally: the earliest JOIN-type
        // node all branch edges converge to (no config string matching).
        let join_node_id = graph.as_ref().and_then(|g| find_join_node(g, &node_id));

        // Pre-generate a branch execution id per path so the branch events
        // and the fork registry carry the branch's own identity.
        let branches: Vec<(Value, wf_types::Id)> = cfg
            .paths
            .iter()
            .map(|p| (p.clone(), wf_common::generate_id()))
            .collect();
        let registry = ctx.fork_registries.get(&node_id).cloned();
        if let Some(registry) = &registry {
            for (path, execution_id) in &branches {
                let path_id = path
                    .get("path_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("path");
                registry.register(path_id, execution_id.clone());
            }
        }

        let runtime = ForkRuntime {
            graph,
            join_node_id: join_node_id.clone(),
            event_bus: event_bus.clone(),
            branch_input: ctx.input.clone(),
            cancellation: ctx.cancellation.clone(),
            fork_registry: registry.clone(),
            branch_ctx: BranchContext {
                handlers,
                event_bus,
                tool_registry: ctx.tool_registry.clone(),
                resource_registries: ctx.resource_registries.clone(),
                parent_variables: ctx.variables.clone(),
                tool_approval_options: ctx.tool_approval_options.clone(),
                tool_approval_handler: ctx.tool_approval_handler.clone(),
                fork_registries: ctx.fork_registries.clone(),
                fork_registry: registry,
            },
            execution_id: execution_id.clone(),
            node_id: node_id.clone(),
        };

        let results: Vec<BranchResult> = if cfg.fork_strategy == "serial" {
            run_serial(&branches, &runtime, cfg.child_execution_timeout).await
        } else if !cfg.wait_for_completion {
            // Non-blocking fork: spawn every branch, keep the task handles
            // in the registry (so the parent can abort them on cancellation)
            // and return immediately. The JOIN node waits for the branches
            // through the same registry.
            spawn_non_blocking(&branches, &runtime, cfg.child_execution_timeout);
            Vec::new()
        } else {
            run_parallel(
                &branches,
                &runtime,
                cfg.child_execution_timeout,
                cfg.total_branch_timeout,
            )
            .await
        };

        let outcome = cfg.failure_strategy.evaluate(&results);

        emit_fork_event(
            runtime.event_bus.as_ref(),
            EventType::ForkCompleted,
            &execution_id,
            HashMap::from([
                (
                    "branch_count".to_string(),
                    Value::Number(serde_json::Number::from(cfg.paths.len() as u64)),
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
            Value::Number(serde_json::Number::from(cfg.paths.len() as u64)),
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

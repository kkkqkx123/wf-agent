use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use wf_core::registry::MutableRegistry;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::types::HookDefinition;
use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_tools::callback::WorkflowOutput;
use wf_types::enums::MiddlewarePhase;
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};
use wf_types::Id;
use wf_workflow::WorkflowCoordinator;
use wf_workflow::entity::WorkflowExecutionEntity;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiError;
use crate::infra::stream::{spawn_execution_stream, ExecutionEventStream};

use super::checkpoint::{attach_checkpoints, entity_resume_snapshot};
use super::graph::resolve_graph;

/// Default wall-clock timeout applied to a workflow execution when the caller
/// does not set `WorkflowExecutionOptions::timeout` and no engine budget is
/// configured (see [`entry_timeout_ms`]).
pub const DEFAULT_EXECUTION_TIMEOUT_MS: u64 = 300_000;

/// Wall-clock budget applied at the API boundary around an execution.
///
/// An explicit `options.timeout` always wins. Otherwise the effective entry
/// budget is the built-in default raised to the engine `max_execution_time`
/// (seeded from the configured execution limits), so the API layer never
/// clamps a run below the budget the engine was configured to honor.
pub(crate) fn entry_timeout_ms(options: &WorkflowExecutionOptions) -> u64 {
    match options.timeout {
        Some(timeout) => timeout,
        None => DEFAULT_EXECUTION_TIMEOUT_MS
            .max(options.max_execution_time.filter(|b| *b > 0).unwrap_or(0)),
    }
}

/// Reserved entity variable holding the resolved execution options so a
/// paused execution can be resumed with the same input/options.
pub(crate) const EXECUTION_OPTIONS_VAR: &str = "__execution_options";

/// Parameters for executing a stored workflow.
#[derive(Debug, Clone, Default)]
pub struct ExecuteWorkflowParams {
    pub workflow_id: String,
    /// Top-level execution input exposed as the `input` variable.
    pub input: Option<Value>,
    /// Execution options; `None` uses engine defaults.
    pub options: Option<WorkflowExecutionOptions>,
}

/// Execute a workflow to completion and await its output.
///
/// Bounded by a wall-clock timeout resolved by [`entry_timeout_ms`]: an
/// explicit `options.timeout` (ms) when set, otherwise the built-in default
/// raised to the engine budget. An elapse maps onto `ApiError::Timeout`.
pub async fn execute(
    ctx: &ApiContext,
    params: ExecuteWorkflowParams,
) -> crate::infra::error::ApiResult<WorkflowOutput> {
    let graph = resolve_graph(ctx, &params.workflow_id).await?;
    let hooks = resolve_hooks(ctx, &params.workflow_id).await?;
    let definition = ctx.storage.workflow.load(&params.workflow_id).await?;
    let entity = spawn_entity(ctx, &params.workflow_id);
    let options = resolve_options(
        ctx,
        &entity,
        definition.as_ref(),
        params.input,
        params.options,
    );
    let timeout_ms = entry_timeout_ms(&options);
    let result = crate::infra::error::with_timeout(
        Duration::from_millis(timeout_ms),
        run_workflow(ctx, entity.clone(), graph, hooks, options),
    )
    .await;
    match result {
        Ok(output) => Ok(output),
        Err(e) => {
            finalize_failed(ctx, &entity).await;
            Err(e)
        }
    }
}

/// Execute a workflow and stream engine events (`WorkflowExecutionStarted`,
/// `NodeStarted`, `NodeCompleted`, `WorkflowExecutionCompleted`, ...)
/// emitted for the execution, ending with `Completed` / `Failed`.
///
/// Returns the generated `execution_id` alongside the stream so the caller
/// can `pause` / `cancel` the backing execution.
pub async fn stream(
    ctx: Arc<ApiContext>,
    params: ExecuteWorkflowParams,
) -> crate::infra::error::ApiResult<(Id, ExecutionEventStream)> {
    let graph = resolve_graph(&ctx, &params.workflow_id).await?;
    let hooks = resolve_hooks(&ctx, &params.workflow_id).await?;
    let definition = ctx.storage.workflow.load(&params.workflow_id).await?;
    let entity = spawn_entity(&ctx, &params.workflow_id);
    let execution_id = entity.id().clone();
    let (stream, sink) =
        spawn_execution_stream(Some(ctx.event_bus.clone()), execution_id.to_string());
    let options = resolve_options(
        &ctx,
        &entity,
        definition.as_ref(),
        params.input,
        params.options,
    );
    let timeout_ms = entry_timeout_ms(&options);
    let execution_key = execution_id.to_string();
    let driver_ctx = ctx.clone();
    let driver_key = execution_key.clone();
    let handle = tokio::spawn(async move {
        let outcome = crate::infra::error::with_timeout(
            Duration::from_millis(timeout_ms),
            run_workflow(&driver_ctx, entity.clone(), graph, hooks, options),
        )
        .await;
        match outcome {
            Ok(output) => {
                let iterations = entity.state.read().await.completed_nodes().len() as u32;
                sink.completed(output.result, iterations).await;
            }
            Err(e) => {
                finalize_failed(&driver_ctx, &entity).await;
                sink.failed(e.to_string()).await;
            }
        }
        driver_ctx.execution_tasks.unregister(&driver_key);
    });
    // Track the detached driver so teardown (`ctx.shutdown`) can abort it
    // instead of letting it run out the execution timeout. The stream also
    // holds the handle and aborts it when the consumer disconnects.
    ctx.execution_tasks
        .register(execution_key.clone(), handle.abort_handle());
    Ok((execution_id, stream.with_task(handle)))
}

/// Pause a running workflow execution (checked between nodes).
pub async fn pause(ctx: &ApiContext, execution_id: &str) -> crate::infra::error::ApiResult<()> {
    let entity = live_entity(ctx, execution_id)?;
    entity.pause().await?;
    // Pause expiry mirrors the agent loop: a detached timer stops an
    // execution that stays paused past its configured budget. The timer
    // re-checks on wake so a timely resume is never disturbed.
    if let Some(max_pause) = execution_options(ctx, &entity).await.max_pause_duration {
        if max_pause > 0 {
            let watched = entity.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(max_pause)).await;
                if watched.state.read().await.status() == ExecutionStatus::Paused {
                    tracing::warn!(
                        execution_id = %watched.id(),
                        max_pause_duration = max_pause,
                        "Workflow execution pause timeout exceeded, stopping execution"
                    );
                    let _ = watched.stop().await;
                }
            });
        }
    }
    Ok(())
}

/// Resume a paused workflow execution and drive it to completion, returning
/// the resumed `WorkflowExecutionResult`.
///
/// The coordinator is re-seeded from the entity's completed node outputs
/// and current node, then runs the remaining graph. The resolved
/// input/options captured at `execute` time are restored automatically.
pub async fn resume(
    ctx: &ApiContext,
    execution_id: &str,
) -> crate::infra::error::ApiResult<WorkflowOutput> {
    let entity = live_entity(ctx, execution_id)?;

    // A completed execution carries its result on the entity (mirrored by
    // the execution callback); return it instead of re-driving a terminal
    // state machine through `start()` (Completed -> Running is illegal).
    if entity.state.read().await.status() == ExecutionStatus::Completed {
        if let Some(result) = entity.output().await {
            return Ok(WorkflowOutput {
                execution_id: entity.id().clone(),
                result,
            });
        }
        return Err(ApiError::Validation(format!(
            "execution {execution_id} already completed without output"
        )));
    }
    if entity.state.read().await.status().is_terminal() {
        return Err(ApiError::Validation(format!(
            "execution {execution_id} is terminal and cannot resume; restore from checkpoint instead"
        )));
    }

    let workflow_id = entity.workflow_id().to_string();
    let graph = resolve_graph(ctx, &workflow_id).await?;

    let options = execution_options(ctx, &entity).await;
    let checkpoints_enabled = options.enable_checkpoints.unwrap_or(true);
    let mut exec_ctx = ExecutorContext::new(
        entity.id().clone(),
        entity.workflow_id().clone(),
        Some(ctx.event_bus.clone()),
        ctx.tool_registry.clone(),
        options,
    )
    .with_resource_registries(ctx.registries.clone());
    if let Some(manager) = entity.hierarchy_manager() {
        exec_ctx = exec_ctx.with_hierarchy_manager(manager);
    }
    exec_ctx.variables = entity.variables().clone();
    if let Some(ref metrics) = ctx.metrics {
        metrics
            .workflow()
            .record_execution_start(entity.workflow_id());
        exec_ctx = exec_ctx.with_metrics(metrics.clone());
    }
    if let Some(ref registry) = ctx.hook_handler_registry {
        exec_ctx = exec_ctx.with_hook_handler_registry(registry.clone());
    }
    attach_host_tool_approval(ctx, &mut exec_ctx, entity.id().as_str());

    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, ctx.handlers())?
        .with_plugin_handlers(ctx.plugin_handlers())
        .with_entity_arc(entity.clone())
        .with_state_manager(ctx.state_manager.clone());
    coordinator = attach_checkpoints(
        coordinator,
        ctx,
        checkpoints_enabled,
        entity.get_hierarchy_depth(),
    );
    let snapshot = entity_resume_snapshot(ctx, &entity).await;
    coordinator.resume_from(&snapshot);

    entity.resume().await?;

    run_lifecycle_middleware(ctx, MiddlewarePhase::BeforeWorkflowExecution, &entity, None).await?;

    match coordinator.execute().await {
        Ok(result) => {
            run_lifecycle_middleware(
                ctx,
                MiddlewarePhase::AfterWorkflowExecution,
                &entity,
                Some(&result),
            )
            .await?;
            Ok(WorkflowOutput {
                execution_id: entity.id().clone(),
                result,
            })
        }
        Err(e) => {
            mark_failed(&entity);
            Err(e.into())
        }
    }
}

/// Cancel (stop) a running workflow execution.
pub async fn cancel(ctx: &ApiContext, execution_id: &str) -> crate::infra::error::ApiResult<()> {
    let entity = live_entity(ctx, execution_id)?;
    entity.stop().await?;
    Ok(())
}

/// Query the live status of a workflow execution.
///
/// Returns the typed [`wf_types::ExecutionStatus`] (the persisted status
/// contract) instead of a Debug string, so callers can match without
/// string parsing. A timeout in the engine state reads as `Timeout`.
pub async fn status(
    ctx: &ApiContext,
    execution_id: &str,
) -> crate::infra::error::ApiResult<wf_types::ExecutionStatus> {
    let entity = live_entity(ctx, execution_id)?;
    let status: wf_types::ExecutionStatus = entity.state.read().await.status().into();
    Ok(status)
}

pub(crate) fn live_entity(
    ctx: &ApiContext,
    execution_id: &str,
) -> crate::infra::error::ApiResult<Arc<WorkflowExecutionEntity>> {
    ctx.workflow_execution(execution_id)
        .ok_or_else(|| ApiError::execution_not_found(execution_id))
}

fn spawn_entity(ctx: &ApiContext, workflow_id: &str) -> Arc<WorkflowExecutionEntity> {
    let execution_id = wf_common::generate_id();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from(execution_id.clone()),
        wf_types::Id::from(workflow_id.to_string()),
    ));
    let _ = ctx
        .workflow_executions
        .register(execution_id, entity.clone());
    entity
}

/// Resolve the effective execution options, storing them on the entity so
/// a later `resume` rebuilds the same input/options.
fn resolve_options(
    ctx: &ApiContext,
    entity: &WorkflowExecutionEntity,
    definition: Option<&wf_types::workflow::WorkflowDefinition>,
    input: Option<Value>,
    options: Option<WorkflowExecutionOptions>,
) -> WorkflowExecutionOptions {
    let mut merged = crate::workflow::composition::resolve_options(definition, input, options);
    // Fill the wall-clock budgets from the configured execution defaults when
    // the caller and definition left them unset, so `limits` is the single
    // source rather than the engine's hardcoded per-node fallback. Explicit
    // caller/definition values always win.
    if let Some(defaults) = ctx
        .execution_limits
        .as_ref()
        .and_then(|limits| limits.execution_defaults.as_ref())
    {
        if merged.node_timeout.is_none() {
            merged.node_timeout = defaults.node_timeout_ms;
        }
        if merged.max_execution_time.is_none() {
            merged.max_execution_time = defaults.max_execution_time_ms;
        }
    }
    if let Ok(value) = serde_json::to_value(&merged) {
        entity.set_variable(EXECUTION_OPTIONS_VAR, value);
    }
    merged
}

/// Reconstruct the execution options captured at `execute` time.
pub(crate) async fn execution_options(
    ctx: &ApiContext,
    entity: &WorkflowExecutionEntity,
) -> WorkflowExecutionOptions {
    let _ = ctx;
    entity
        .get_variable(EXECUTION_OPTIONS_VAR)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_else(default_options)
}

/// Host-default tool approval: when the context enables it and the
/// execution carries no caller-supplied wiring, route the tool calls the
/// policy marks as `Ask` (across agent-loop, fork-join and plain nodes)
/// through the persisted interaction flow. A no-op keeps the library
/// default (auto-approve) untouched.
fn attach_host_tool_approval(ctx: &ApiContext, exec_ctx: &mut ExecutorContext, execution_id: &str) {
    if exec_ctx.tool_approval_handler.is_some() || exec_ctx.tool_approval_options.is_some() {
        return;
    }
    if let Some(wiring) =
        crate::workflow::tool_approval_handler::host_tool_approval(ctx, execution_id)
    {
        exec_ctx.tool_approval_options = Some(wiring.options);
        exec_ctx.tool_approval_handler = Some(wiring.handler);
    }
}

/// Plugin lifecycle middleware around workflow executions: run every plugin
/// middleware registered for `phase` in priority order. The context payload
/// carries the execution id, workflow id and the workflow input / final
/// output (when one exists).
async fn run_lifecycle_middleware(
    ctx: &ApiContext,
    phase: MiddlewarePhase,
    entity: &WorkflowExecutionEntity,
    payload: Option<&Value>,
) -> crate::infra::error::ApiResult<()> {
    let context = serde_json::json!({
        "execution_id": entity.id().as_str(),
        "workflow_id": entity.workflow_id().as_str(),
        "payload": payload.cloned().unwrap_or(Value::Null),
    });
    ctx.run_middleware(phase, &context).await
}

/// Run a workflow against the shared context, driving the entity so
/// external `pause` / `resume` / `cancel` calls apply to the live execution.
/// The coordinator persists the `WorkflowExecution` record through the shared
/// state manager at start and on every terminal exit; `execute` / `stream`
/// additionally finalize the record when a wall-clock timeout drops the
/// coordinator before it can write one.
async fn run_workflow(
    ctx: &ApiContext,
    entity: Arc<WorkflowExecutionEntity>,
    graph: WorkflowGraphStructure,
    hooks: Vec<HookDefinition>,
    options: WorkflowExecutionOptions,
) -> crate::infra::error::ApiResult<WorkflowOutput> {
    let checkpoints_enabled = options.enable_checkpoints.unwrap_or(true);
    let workflow_input = options.input.clone();
    let mut exec_ctx = ExecutorContext::new(
        entity.id().clone(),
        entity.workflow_id().clone(),
        Some(ctx.event_bus.clone()),
        ctx.tool_registry.clone(),
        options,
    )
    .with_resource_registries(ctx.registries.clone());
    if let Some(manager) = entity.hierarchy_manager() {
        exec_ctx = exec_ctx.with_hierarchy_manager(manager);
    }
    exec_ctx.variables = entity.variables().clone();
    if let Some(ref metrics) = ctx.metrics {
        metrics
            .workflow()
            .record_execution_start(entity.workflow_id());
        exec_ctx = exec_ctx.with_metrics(metrics.clone());
    }
    attach_host_tool_approval(ctx, &mut exec_ctx, entity.id().as_str());

    // Plugin lifecycle middleware (`BeforeWorkflowExecution`) runs before the
    // state machine starts; a middleware failure aborts the execution.
    run_lifecycle_middleware(
        ctx,
        MiddlewarePhase::BeforeWorkflowExecution,
        &entity,
        workflow_input.as_ref(),
    )
    .await?;

    let _ = entity.state.write().await.start();

    // Node hooks publish HOOK_TRIGGERED events on the shared event bus
    // (carried by the execution context).
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, ctx.handlers())?
        .with_plugin_handlers(ctx.plugin_handlers())
        .with_entity_arc(entity.clone())
        .with_state_manager(ctx.state_manager.clone())
        .with_hooks(hooks);
    coordinator = attach_checkpoints(
        coordinator,
        ctx,
        checkpoints_enabled,
        entity.get_hierarchy_depth(),
    );

    let result = coordinator.execute().await;
    run_lifecycle_middleware(
        ctx,
        MiddlewarePhase::AfterWorkflowExecution,
        &entity,
        result.as_ref().ok(),
    )
    .await?;
    match result {
        Ok(output) => {
            // Mirror the final outcome onto the entity exposed through the
            // registry so later `resume` / status queries observe the
            // settled result without re-driving the terminal state machine.
            entity.set_output(output.clone()).await;
            Ok(WorkflowOutput {
                execution_id: entity.id().clone(),
                result: output,
            })
        }
        Err(e) => Err(e.into()),
    }
}

/// Load the definition-level hooks of a stored workflow, converted into
/// executable hook definitions.
async fn resolve_hooks(
    ctx: &ApiContext,
    workflow_id: &str,
) -> crate::infra::error::ApiResult<Vec<HookDefinition>> {
    let definition = ctx
        .storage
        .workflow
        .load(workflow_id)
        .await?
        .ok_or_else(|| crate::infra::error::not_found("workflow", workflow_id))?;
    Ok(definition
        .hooks
        .as_ref()
        .map(|hooks| hooks.iter().map(Into::into).collect())
        .unwrap_or_default())
}

pub(crate) fn default_options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(true),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

fn mark_failed(entity: &WorkflowExecutionEntity) {
    if let Ok(mut state) = entity.state.try_write() {
        let _ = state.fail("execution failed".to_string());
    }
}

/// Finalize a workflow execution that left the coordinator before writing a
/// terminal record — a wall-clock timeout drops the coordinator mid-run, so
/// the start record stays `Running`. Marks the entity failed and flips the
/// persisted record to `Failed` to match.
async fn finalize_failed(ctx: &ApiContext, entity: &WorkflowExecutionEntity) {
    mark_failed(entity);
    ctx.state_manager
        .update_workflow_status(entity.id(), &wf_types::ExecutionStatus::Failed)
        .await;
}

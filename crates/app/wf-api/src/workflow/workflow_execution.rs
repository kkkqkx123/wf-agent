use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
use wf_checkpoint::coordinator::CheckpointCoordinator;
use wf_checkpoint::state::WorkflowCheckpointStateManager;
use wf_common::retry::{RetryBudget, RetryBudgetConfig, TimeBudgetMode};
use wf_core::registry::MutableRegistry;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_tools::callback::WorkflowOutput;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::{CheckpointTiming, CheckpointVariableState};
use wf_types::execution::{ChildExecutionReference, ExecutionHierarchy, ExecutionType};
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_types::Id;
use wf_workflow::checkpoint::{NodeCheckpointStrategy, WorkflowCheckpointIntegration};
use wf_workflow::entity::WorkflowExecutionEntity;
use wf_workflow::validation::GraphValidator;
use wf_workflow::WorkflowCoordinator;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError};
use crate::infra::stream::{spawn_execution_stream, ExecutionEventStream};

/// Default wall-clock timeout applied to a workflow execution when the caller
/// does not set `WorkflowExecutionOptions::timeout`.
pub const DEFAULT_EXECUTION_TIMEOUT_MS: u64 = 300_000;

/// Reserved entity variable holding the resolved execution options so a
/// paused execution can be resumed with the same input/options.
const EXECUTION_OPTIONS_VAR: &str = "__execution_options";

/// Parameters for executing a stored workflow.
#[derive(Debug, Clone, Default)]
pub struct ExecuteWorkflowParams {
    pub workflow_id: String,
    /// Top-level execution input exposed as the `input` variable.
    pub input: Option<Value>,
    /// Execution options; `None` uses engine defaults.
    pub options: Option<WorkflowExecutionOptions>,
}

/// Restored execution state returned by [`crate::workflow::workflow_execution::restore_checkpoint`].
///
/// Upgraded from a snapshot view into a runnable restore result: the restored
/// snapshot's `current_node_id` / `node_results` / `variable_state` are
/// backfilled into a fresh live entity registered in the context, so the
/// restored execution can be driven to completion through
/// [`crate::workflow::workflow_execution::resume`] / [`crate::workflow::workflow_execution::restore_and_resume`].
#[derive(Clone, Serialize)]
pub struct RestoredCheckpoint {
    pub checkpoint_id: String,
    pub execution_id: String,
    pub status: String,
    pub current_node_id: Option<String>,
    pub node_results: Option<HashMap<String, Value>>,
    pub variables: BTreeMap<String, Value>,
    /// Restored live entity (registered in the context under `execution_id`)
    /// whose state was backfilled from the checkpoint.
    #[serde(skip)]
    pub entity: Arc<WorkflowExecutionEntity>,
}

/// Application-facing workflow execution API.
///
/// Launches executions of stored workflows through the `wf-workflow` engine,
/// keeps a live entity handle in the context so `pause` / `resume` / `cancel`
/// and status queries work while the coordinator drives the same entity.
/// Load a stored workflow definition and convert it into an executable
/// graph, running shape plus graph plus reference closure validation.
/// Stored workflows that went stale after an upstream change fail here
/// explicitly instead of failing piecemeal at node runtime.
pub async fn resolve_graph(
    ctx: &ApiContext,
    workflow_id: &str,
) -> crate::infra::error::ApiResult<WorkflowGraphStructure> {
    if ctx.is_stale(workflow_id) {
        tracing::warn!(
            workflow_id = %workflow_id,
            "workflow is marked stale after an upstream update; revalidation runs before execution"
        );
    }
    let definition = ctx
        .storage
        .workflow
        .load(workflow_id)
        .await?
        .ok_or_else(|| not_found("workflow", workflow_id))?;
    let graph = definition_to_graph(&definition);
    let val_ctx = crate::workflow::validation::build_reference_context(ctx).await;
    let ref_ctx = crate::workflow::validation::val_ctx_to_reference_context(&val_ctx);
    let (validated, warnings) = GraphValidator::validate_with_reference_context(graph, &ref_ctx)
        .map_err(|errors| {
            let detail = errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            ApiError::Validation(format!(
                "workflow reference closure failed ({} error(s)): {}",
                errors.len(),
                detail
            ))
        })?;
    if !warnings.is_empty() {
        tracing::warn!(
            workflow_id = %workflow_id,
            warnings = %warnings.iter().map(|w| format!("{}: {}", w.field, w.message)).collect::<Vec<_>>().join("; "),
            "workflow executes with reference warnings"
        );
    }
    let tool_report = wf_workflow::reference_closure::validate_workflow_tool_lists(
        workflow_id,
        definition.available_tools.as_ref(),
        &ref_ctx,
    );
    if !tool_report.errors.is_empty() {
        let detail = tool_report
            .errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ApiError::Validation(format!(
            "workflow reference closure failed ({} error(s)): {}",
            tool_report.errors.len(),
            detail
        )));
    }
    Ok(validated.into_inner())
}

/// Execute a workflow to completion and await its output.
///
/// Bounded by a wall-clock timeout: `options.timeout` (ms) when set,
/// otherwise [`DEFAULT_EXECUTION_TIMEOUT_MS`] (5min). An elapse maps onto
/// `ApiError::Timeout`.
pub async fn execute(
    ctx: &ApiContext,
    params: ExecuteWorkflowParams,
) -> crate::infra::error::ApiResult<WorkflowOutput> {
    let graph = resolve_graph(ctx, &params.workflow_id).await?;
    let hooks = resolve_hooks(ctx, &params.workflow_id).await?;
    let entity = spawn_entity(ctx, &params.workflow_id);
    let options = resolve_options(ctx, &entity, params.input, params.options);
    let timeout_ms = options.timeout.unwrap_or(DEFAULT_EXECUTION_TIMEOUT_MS);
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
    let entity = spawn_entity(&ctx, &params.workflow_id);
    let execution_id = entity.id().clone();
    let (stream, sink) =
        spawn_execution_stream(Some(ctx.event_bus.clone()), execution_id.to_string());
    let options = resolve_options(&ctx, &entity, params.input, params.options);
    let timeout_ms = options.timeout.unwrap_or(DEFAULT_EXECUTION_TIMEOUT_MS);
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
    entity.interruption().pause()?;
    entity.state.write().await.pause()?;
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
    }

    let workflow_id = entity.workflow_id().to_string();
    let graph = resolve_graph(ctx, &workflow_id).await?;

    let options = execution_options(ctx, &entity).await;
    let checkpoints_enabled = options.enable_checkpoints.unwrap_or(true);
    let retry_budget = build_retry_budget(&options, ctx.metrics.as_ref());
    let mut exec_ctx = ExecutorContext::new(
        entity.id().clone(),
        entity.workflow_id().clone(),
        Some(ctx.event_bus.clone()),
        ctx.tool_registry.clone(),
        options,
    )
    .with_resource_registries(ctx.registries.clone());
    if let Some(budget) = &retry_budget {
        exec_ctx = exec_ctx.with_retry_budget(budget.clone());
    }
    exec_ctx.variables = entity.variables().clone();
    if let Some(ref metrics) = ctx.metrics {
        metrics
            .workflow()
            .record_execution_start(entity.workflow_id());
        exec_ctx = exec_ctx.with_metrics(metrics.clone());
    }
    if let Some(ref registry) = ctx.hook_registry {
        exec_ctx = exec_ctx.with_hook_registry(registry.clone());
    }
    attach_host_tool_approval(ctx, &mut exec_ctx, entity.id().as_str());

    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, ctx.handlers())?
        .with_entity_arc(entity.clone())
        .with_state_manager(ctx.state_manager.clone());
    coordinator = attach_checkpoints(coordinator, ctx, checkpoints_enabled);
    let snapshot = entity_resume_snapshot(&entity).await;
    coordinator.resume_from(&snapshot);

    let _ = entity.state.write().await.resume();

    match coordinator.execute().await {
        Ok(result) => Ok(WorkflowOutput {
            execution_id: entity.id().clone(),
            result,
        }),
        Err(e) => {
            mark_failed(&entity);
            Err(e.into())
        }
    }
}

/// Create an execution checkpoint for a live workflow execution.
///
/// The snapshot is built from the entity's current variables, node
/// results and state, and persisted through the `wf-checkpoint`
/// coordinator onto `ctx.checkpoint_store`. Checkpoint commands only
/// take effect for persistent stores; the default in-memory store keeps
/// checkpoints for the process lifetime.
pub async fn create_checkpoint(
    ctx: &ApiContext,
    execution_id: &str,
) -> crate::infra::error::ApiResult<String> {
    let entity = live_entity(ctx, execution_id)?;
    let snapshot = build_checkpoint_snapshot(ctx, &entity).await;
    let coordinator = checkpoint_coordinator(ctx);
    let checkpoint_id = coordinator
        .create_checkpoint(CheckpointTiming::Manual, execution_id, snapshot)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint creation failed: {e}")))?;
    Ok(checkpoint_id)
}

/// Restore execution state from a checkpoint.
///
/// The checkpoint's `current_node_id` / `node_results` / `variable_state` are
/// backfilled into a fresh live entity (registered under the checkpoint's
/// execution id) together with the captured execution options, so the
/// restored execution continues through the standard [`resume`] coordinator
/// path.
pub async fn restore_checkpoint(
    ctx: &ApiContext,
    checkpoint_id: &str,
) -> crate::infra::error::ApiResult<RestoredCheckpoint> {
    let coordinator = checkpoint_coordinator(ctx);
    let restored = coordinator
        .restore(checkpoint_id)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint restore failed: {e}")))?;
    let snapshot = restored.snapshot;
    let execution_id = snapshot.execution_id.clone();

    // Resolve the workflow identity captured at checkpoint time; fall back
    // to the persisted execution record for older checkpoints.
    let (workflow_id, options) = restored_identity(ctx, &snapshot).await?;

    // Build a fresh live entity backfilled from the restored snapshot.
    let mut entity =
        WorkflowExecutionEntity::new(snapshot.execution_id.clone(), workflow_id.clone());
    if let Some(hierarchy) = snapshot.hierarchy.as_ref() {
        if let Some(parent_id) = hierarchy.parent_execution_id.clone() {
            entity = entity.with_parent_execution_id(parent_id);
        }
        // Carry the persisted ancestor chain across the process boundary so
        // deep hierarchies keep full ancestry after restore.
        if let Some(ancestors) = hierarchy.ancestors.clone() {
            entity = entity.with_ancestors(ancestors);
        }
    }
    let entity = Arc::new(entity);
    for (name, value) in &snapshot.variable_state.variables {
        entity.set_variable(name.clone(), value.clone());
    }
    if let Some(node_results) = &snapshot.node_results {
        for (node_id, output) in node_results {
            entity.set_node_result(node_id.clone(), output.clone());
        }
    }
    if let Some(node_id) = &snapshot.current_node_id {
        entity
            .state
            .write()
            .await
            .set_current_node(Some(node_id.clone()));
    }
    // Replay the captured per-node audit trail so post-restore audit queries
    // keep the full history.
    if let Some(records) = &snapshot.node_execution_records {
        let history: Vec<wf_workflow::state::NodeExecutionRecord> = records
            .iter()
            .map(|record| wf_workflow::state::NodeExecutionRecord {
                node_id: record.node_id.clone(),
                node_name: record.node_id.clone(),
                node_type: record.node_type.clone(),
                start_time: record.started_at,
                end_time: record.completed_at,
                success: record.error.is_none(),
                error: record.error.clone(),
                input: record.input.clone(),
                result: record.result.clone(),
                branch_id: record.branch_id.clone(),
            })
            .collect();
        entity
            .state
            .write()
            .await
            .restore_node_execution_history(history);
    }
    // Restore the captured execution options so `resume` rebuilds the same
    // input/options. Step/time budgets already consumed by the original run
    // are not re-applied to the continuation (a restored continuation
    // must run to completion, not re-limit itself at the old budget).
    let mut continuation_options = options;
    continuation_options.max_steps = None;
    continuation_options.max_execution_time = None;
    if let Ok(value) = serde_json::to_value(&continuation_options) {
        entity.set_variable(EXECUTION_OPTIONS_VAR, value);
    }

    // Replace any stale live handle under the restored execution id.
    let key = execution_id.to_string();
    let _ = ctx.workflow_executions.unregister(&key);
    let _ = ctx
        .workflow_executions
        .register(key.clone(), entity.clone());

    let mut variables = BTreeMap::new();
    for (name, value) in &snapshot.variable_state.variables {
        variables.insert(name.clone(), value.clone());
    }
    Ok(RestoredCheckpoint {
        checkpoint_id: checkpoint_id.to_string(),
        execution_id: key,
        status: restored.status,
        current_node_id: snapshot.current_node_id,
        node_results: snapshot.node_results,
        variables,
        entity,
    })
}

/// Restore execution state from a checkpoint and immediately drive it to
/// completion, returning the final output.
///
/// Equivalent to [`restore_checkpoint`] followed by [`resume`] on the
/// restored execution.
pub async fn restore_and_resume(
    ctx: &ApiContext,
    checkpoint_id: &str,
) -> crate::infra::error::ApiResult<WorkflowOutput> {
    let restored = restore_checkpoint(ctx, checkpoint_id).await?;
    resume(ctx, &restored.execution_id).await
}

/// Cancel (stop) a running workflow execution.
pub async fn cancel(ctx: &ApiContext, execution_id: &str) -> crate::infra::error::ApiResult<()> {
    let entity = live_entity(ctx, execution_id)?;
    entity.interruption().stop()?;
    let _ = entity.state.write().await.cancel();
    Ok(())
}

/// Query the live status of a workflow execution.
///
/// Returns the typed [`wf_types::ExecutionStatus`] (the persisted status
/// contract) instead of a Debug string, so callers can match without
/// string parsing. A timeout in the engine state reads as `Failed`.
pub async fn status(
    ctx: &ApiContext,
    execution_id: &str,
) -> crate::infra::error::ApiResult<wf_types::ExecutionStatus> {
    let entity = live_entity(ctx, execution_id)?;
    let status: wf_types::ExecutionStatus = entity.state.read().await.status().into();
    Ok(status)
}

fn live_entity(
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
    input: Option<Value>,
    options: Option<WorkflowExecutionOptions>,
) -> WorkflowExecutionOptions {
    let _ = ctx;
    let mut options = options.unwrap_or_else(default_options);
    if options.input.is_none() {
        options.input = input;
    }
    if let Ok(value) = serde_json::to_value(&options) {
        entity.set_variable(EXECUTION_OPTIONS_VAR, value);
    }
    options
}

/// Reconstruct the execution options captured at `execute` time.
async fn execution_options(
    ctx: &ApiContext,
    entity: &WorkflowExecutionEntity,
) -> WorkflowExecutionOptions {
    let _ = ctx;
    entity
        .get_variable(EXECUTION_OPTIONS_VAR)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_else(default_options)
}

/// Map the entity's recorded node execution history into the checkpoint
/// snapshot's audit records. Returns `None` when nothing was
/// recorded, keeping older blobs byte-compatible with the new field.
fn snapshot_node_records(
    state: &wf_workflow::state::WorkflowExecutionState,
) -> Option<Vec<wf_types::checkpoint::workflow::NodeExecutionRecord>> {
    let history = state.node_execution_history();
    if history.is_empty() {
        return None;
    }
    Some(
        history
            .iter()
            .map(
                |record| wf_types::checkpoint::workflow::NodeExecutionRecord {
                    node_id: record.node_id.clone(),
                    node_type: record.node_type.clone(),
                    input: record.input.clone(),
                    result: record.result.clone(),
                    error: record.error.clone(),
                    started_at: record.start_time,
                    completed_at: record.end_time,
                    duration_ms: record
                        .end_time
                        .map(|end| end - record.start_time)
                        .unwrap_or(0),
                    branch_id: record.branch_id.clone(),
                },
            )
            .collect(),
    )
}

/// Build a resume snapshot from the entity's current completion state:
/// node results seed the coordinator's outputs and completed set, the
/// current node restarts execution from there.
async fn entity_resume_snapshot(
    entity: &WorkflowExecutionEntity,
) -> WorkflowExecutionStateSnapshot {
    let state = entity.state.read().await;
    let mut node_results = HashMap::new();
    for entry in entity.node_results().iter() {
        node_results.insert(entry.key().clone(), entry.value().clone());
    }
    let mut variables = HashMap::new();
    for entry in entity.variables().iter() {
        if entry.key() != EXECUTION_OPTIONS_VAR {
            variables.insert(entry.key().clone(), entry.value().clone());
        }
    }
    let node_execution_records = snapshot_node_records(&state);
    WorkflowExecutionStateSnapshot {
        execution_id: entity.id().to_string(),
        status: state.status().as_str().to_string(),
        current_node_id: state.current_node_id().map(String::from),
        node_results: Some(node_results),
        node_execution_records,
        variable_state: CheckpointVariableState { variables },
        input: None,
        output: None,
        messages: None,
        fork_join_context: None,
        active_operations: None,
        conversation_state: None,
        trigger_states: None,
        error_records: None,
        interruption_records: None,
        event_records: None,
        hierarchy: None,
        execution_config: None,
        fork_join_aggregation_state: None,
        hook_execution_context: None,
        message_base_checkpoint_id: None,
        message_total_count: None,
        truncated: None,
        truncation_stats: None,
    }
}

/// Build a full checkpoint snapshot from the entity's live state.
///
/// Enriched beyond the resume-view snapshot so a cross-process restore
/// reconstructs a runnable execution: the captured execution options
/// (input + options, used to rebuild the `ExecutorContext`), the execution
/// hierarchy (parent/children linkage) and the recorded error records are
/// all persisted. `fork_join_context` is not tracked on the entity and
/// stays `None`; the engine tracks fork/join only transiently.
async fn build_checkpoint_snapshot(
    ctx: &ApiContext,
    entity: &WorkflowExecutionEntity,
) -> WorkflowExecutionStateSnapshot {
    let state = entity.state.read().await;
    let options = execution_options(ctx, entity).await;
    let mut variables = HashMap::new();
    for entry in entity.variables().iter() {
        if entry.key() != EXECUTION_OPTIONS_VAR {
            variables.insert(entry.key().clone(), entry.value().clone());
        }
    }
    let mut node_results = HashMap::new();
    for entry in entity.node_results().iter() {
        node_results.insert(entry.key().clone(), entry.value().clone());
    }
    let error_records = if state.error_records().is_empty() {
        None
    } else {
        Some(
            state
                .error_records()
                .iter()
                .filter_map(|r| serde_json::to_value(r).ok())
                .collect(),
        )
    };
    let active_operations = state.operation_state().map(|op| vec![op.clone()]);
    let hierarchy = build_hierarchy(entity).await;
    let node_execution_records = snapshot_node_records(&state);
    // Trigger audit trail: which event-driven triggers fired for this
    // execution and their run status.
    let trigger_states = ctx
        .trigger_state_registry
        .snapshot_for(entity.id().as_str());

    WorkflowExecutionStateSnapshot {
        execution_id: entity.id().to_string(),
        status: state.status().as_str().to_string(),
        current_node_id: state.current_node_id().map(String::from),
        node_results: Some(node_results),
        node_execution_records,
        variable_state: CheckpointVariableState { variables },
        input: options.input.clone(),
        output: None,
        messages: None,
        fork_join_context: None,
        active_operations,
        conversation_state: None,
        trigger_states,
        error_records,
        interruption_records: None,
        event_records: None,
        hierarchy,
        execution_config: Some(serde_json::json!({
            "workflow_id": entity.workflow_id().to_string(),
            "options": options,
        })),
        fork_join_aggregation_state: None,
        hook_execution_context: None,
        message_base_checkpoint_id: None,
        message_total_count: None,
        truncated: None,
        truncation_stats: None,
    }
}

/// Build the execution hierarchy captured at checkpoint time: the
/// execution's workflow id / id plus the parent linkage and any registered
/// child executions (fork paths / sub-workflows). Child types are not
/// tracked on the entity, so they default to `Workflow`.
async fn build_hierarchy(entity: &WorkflowExecutionEntity) -> Option<ExecutionHierarchy> {
    let children = entity.child_execution_ids().read().await.clone();
    let has_children = !children.is_empty();
    let parent = entity.parent_execution_id().cloned();
    let ancestors = entity.ancestors();
    if parent.is_none() && !has_children {
        return None;
    }
    Some(ExecutionHierarchy {
        workflow_id: entity.workflow_id().clone(),
        execution_id: entity.id().clone(),
        parent_execution_id: parent,
        depth: 0,
        root_execution_id: None,
        ancestors: if ancestors.is_empty() {
            None
        } else {
            Some(ancestors.to_vec())
        },
        children: if has_children {
            Some(
                children
                    .into_iter()
                    .map(|child_id| ChildExecutionReference {
                        child_type: ExecutionType::Workflow,
                        child_id,
                        created_at: wf_common::now(),
                        fork_path_id: None,
                    })
                    .collect(),
            )
        } else {
            None
        },
    })
}

/// Resolve the workflow identity and execution options needed to rebuild a
/// runnable execution from a restored snapshot. Prefers the identity
/// captured at checkpoint time (`execution_config.workflow_id` +
/// `execution_config.options`); for checkpoints created before the
/// enrichment, falls back to the persisted execution record and
/// default options.
async fn restored_identity(
    ctx: &ApiContext,
    snapshot: &WorkflowExecutionStateSnapshot,
) -> crate::infra::error::ApiResult<(wf_types::Id, WorkflowExecutionOptions)> {
    if let Some(config) = &snapshot.execution_config {
        if let Some(workflow_id) = config.get("workflow_id").and_then(|v| v.as_str()) {
            let options = config
                .get("options")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_else(default_options);
            return Ok((wf_types::Id::from(workflow_id.to_string()), options));
        }
    }
    if let Ok(Some(record)) = ctx
        .storage
        .workflow_execution
        .load(&snapshot.execution_id)
        .await
    {
        return Ok((record.workflow_id, default_options()));
    }
    Err(ApiError::execution(format!(
        "cannot resolve workflow id for restored execution {}",
        snapshot.execution_id
    )))
}

/// Build a `wf-checkpoint` workflow coordinator over the shared
/// checkpoint store (create and restore commands).
fn checkpoint_coordinator(ctx: &ApiContext) -> WorkflowCheckpointCoordinator {
    let state_manager = WorkflowCheckpointStateManager::new(ctx.checkpoint_store.clone());
    let mut coordinator = WorkflowCheckpointCoordinator::new(state_manager);
    if let Some(manager) = ctx.file_checkpoint_manager() {
        coordinator = coordinator.with_file_checkpoint_manager(manager.clone());
    }
    coordinator
}

/// Build the checkpoint integration mounted onto the coordinator of every
/// execution that has checkpoints enabled. The default strategy snapshots
/// after every completed node plus an unconditional start/end snapshot
/// (start/end are not strategy-gated).
fn checkpoint_integration(ctx: &ApiContext) -> WorkflowCheckpointIntegration {
    let mut integration = WorkflowCheckpointIntegration::new(
        ctx.checkpoint_store.clone(),
        NodeCheckpointStrategy::default(),
    )
    .with_core_event_bus(ctx.event_bus.clone());
    if let Some(manager) = ctx.file_checkpoint_manager() {
        integration = integration.with_file_checkpoint_manager(manager.clone());
    }
    integration
}

/// Attach the checkpoint integration when `enable_checkpoints` is not
/// explicitly disabled.
fn attach_checkpoints(
    coordinator: WorkflowCoordinator,
    ctx: &ApiContext,
    checkpoints_enabled: bool,
) -> WorkflowCoordinator {
    if checkpoints_enabled {
        coordinator.with_checkpoint(checkpoint_integration(ctx))
    } else {
        coordinator
    }
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

/// Run a workflow against the shared context, driving the given entity so
/// external `pause` / `resume` / `cancel` calls apply to the live execution.
/// The coordinator persists the `WorkflowExecution` record through the shared
/// state manager at start and on every terminal exit; `execute` / `stream`
/// additionally finalize the record when a wall-clock timeout drops the
/// coordinator before it can write one.
async fn run_workflow(
    ctx: &ApiContext,
    entity: Arc<WorkflowExecutionEntity>,
    graph: WorkflowGraphStructure,
    hooks: Vec<BaseHookDefinition>,
    options: WorkflowExecutionOptions,
) -> crate::infra::error::ApiResult<WorkflowOutput> {
    let checkpoints_enabled = options.enable_checkpoints.unwrap_or(true);
    let retry_budget = build_retry_budget(&options, ctx.metrics.as_ref());
    let mut exec_ctx = ExecutorContext::new(
        entity.id().clone(),
        entity.workflow_id().clone(),
        Some(ctx.event_bus.clone()),
        ctx.tool_registry.clone(),
        options,
    )
    .with_resource_registries(ctx.registries.clone());
    if let Some(budget) = &retry_budget {
        exec_ctx = exec_ctx.with_retry_budget(budget.clone());
    }
    exec_ctx.variables = entity.variables().clone();
    if let Some(ref metrics) = ctx.metrics {
        metrics
            .workflow()
            .record_execution_start(entity.workflow_id());
        exec_ctx = exec_ctx.with_metrics(metrics.clone());
    }
    attach_host_tool_approval(ctx, &mut exec_ctx, entity.id().as_str());

    let _ = entity.state.write().await.start();

    // Node hooks publish HOOK_TRIGGERED events on the shared event bus
    // (carried by the execution context).
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, ctx.handlers())?
        .with_entity_arc(entity.clone())
        .with_state_manager(ctx.state_manager.clone())
        .with_hooks(hooks);
    coordinator = attach_checkpoints(coordinator, ctx, checkpoints_enabled);

    let result = coordinator.execute().await;
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
) -> crate::infra::error::ApiResult<Vec<BaseHookDefinition>> {
    let definition = ctx
        .storage
        .workflow
        .load(workflow_id)
        .await?
        .ok_or_else(|| not_found("workflow", workflow_id))?;
    Ok(definition
        .hooks
        .as_ref()
        .map(|hooks| hooks.iter().map(Into::into).collect())
        .unwrap_or_default())
}

fn default_options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(true),
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
    }
}

/// Shared retry budget created from `WorkflowExecutionOptions::retry_budget`.
/// `None` keeps the historical no-budget behavior (each retry path applies
/// its own limits without a global accounting). When a metrics registry is
/// present, budget events are wired into the `RetryBudgetMetricsCollector`.
fn build_retry_budget(
    options: &WorkflowExecutionOptions,
    metrics: Option<&Arc<wf_metrics::MetricsRegistry>>,
) -> Option<Arc<RetryBudget>> {
    options.retry_budget.as_ref().map(|opt| {
        let on_event = metrics.map(|m| {
            wf_metrics::RetryBudgetMetricsCollector::event_handler(
                m.retry_budget(),
                "workflow".to_string(),
            )
        });
        Arc::new(RetryBudget::new(RetryBudgetConfig {
            max_retries: opt.max_retries,
            time_budget_ms: opt.time_budget_ms,
            time_budget_mode: TimeBudgetMode::default(),
            name: "workflow".to_string(),
            on_event,
        }))
    })
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

/// Convert a stored [`WorkflowDefinition`] into an executable graph.
///
/// Nodes map their `config` onto the flattened `inner` field consumed by the
/// node handlers; edges map directly. Graph boundaries are derived from node
/// types (the START / END nodes, or the message pair for triggered
/// subgraphs): node order carries no semantics. A definition without a
/// boundary node yields an empty boundary and fails graph validation.
pub fn definition_to_graph(
    definition: &wf_types::workflow::WorkflowDefinition,
) -> WorkflowGraphStructure {
    use wf_types::node::StaticNodeType;
    let nodes: Vec<WorkflowNode> = definition
        .nodes
        .iter()
        .map(|node| WorkflowNode {
            id: node.id.clone(),
            name: node.name.clone(),
            node_type: node_type_string(&node.node_type),
            inner: node_inner_config(node),
        })
        .collect();
    let edges: Vec<WorkflowEdge> = definition
        .edges
        .iter()
        .map(|edge| WorkflowEdge {
            id: edge.id.clone(),
            source_node_id: edge.source_node_id.clone(),
            target_node_id: edge.target_node_id.clone(),
            r#type: edge.r#type.clone(),
            condition: edge.condition.clone(),
            label: edge.label.clone(),
            description: edge.description.clone(),
        })
        .collect();
    WorkflowGraphStructure {
        start_node_id: definition
            .nodes
            .iter()
            .find(|n| n.node_type == StaticNodeType::Start)
            .or_else(|| {
                definition
                    .nodes
                    .iter()
                    .find(|n| n.node_type == StaticNodeType::StartFromMessage)
            })
            .map(|n| n.id.clone()),
        end_node_ids: {
            let ends: Vec<String> = definition
                .nodes
                .iter()
                .filter(|n| n.node_type == StaticNodeType::End)
                .map(|n| n.id.clone())
                .collect();
            if ends.is_empty() {
                definition
                    .nodes
                    .iter()
                    .filter(|n| n.node_type == StaticNodeType::ContinueFromMessage)
                    .map(|n| n.id.clone())
                    .collect()
            } else {
                ends
            }
        },
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
    }
}

fn node_type_string(node_type: &wf_types::node::StaticNodeType) -> String {
    serde_json::to_string(node_type)
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_default()
}

/// Node config plus execution config merged into a single JSON object
/// (execution config wins on conflicts). The runtime reads node-level
/// behavior (`checkpoint`, `timeout_seconds`, retry policy, ...) from the
/// node's `inner` blob, so the typed `execution_config` surface is merged
/// here instead of being dropped at graph conversion.
fn node_inner_config(node: &wf_types::node::BaseStaticNode) -> Value {
    match (&node.config, &node.execution_config) {
        (Some(config), Some(execution_config)) => {
            let mut map = match config {
                Value::Object(m) => m.clone(),
                _ => serde_json::Map::new(),
            };
            map.extend(execution_config.config_fields());
            Value::Object(map)
        }
        (Some(config), None) => config.clone(),
        (None, Some(execution_config)) => Value::Object(execution_config.config_fields()),
        (None, None) => Value::Null,
    }
}

/// Digest of a workflow execution: the fields most execution-list consumers
/// read, computed from the persisted record.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionSummary {
    pub id: String,
    pub workflow_id: String,
    pub status: wf_types::ExecutionStatus,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    /// Elapsed time in ms (`completed_at - started_at`; `None` while running).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<i64>,
    pub error_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Project [`crate::workflow::list_executions`] results onto
/// [`ExecutionSummary`].
pub async fn execution_summaries(
    ctx: &ApiContext,
    options: Option<wf_storage::adapter::execution::WorkflowExecutionListOptions>,
) -> crate::ApiResult<Vec<ExecutionSummary>> {
    Ok(crate::workflow::list_executions(ctx, options)
        .await?
        .into_iter()
        .map(|execution| {
            let error_count = execution.errors.as_ref().map(|e| e.len()).unwrap_or(0);
            let elapsed_ms = match (execution.started_at, execution.completed_at) {
                (start, Some(end)) => Some(end - start),
                _ => None,
            };
            ExecutionSummary {
                id: execution.id.clone(),
                workflow_id: execution.workflow_id.clone(),
                status: execution.status.clone(),
                started_at: execution.started_at,
                completed_at: execution.completed_at,
                elapsed_ms,
                error_count,
                error: execution.error.clone(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::node::BaseStaticNode;
    use wf_types::node::StaticNodeType;
    use wf_types::workflow::edge::EdgeType;
    use wf_types::workflow::WorkflowDefinition;

    fn make_definition(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: None,
            r#type: None,
            version: Some("1.0.0".into()),
            nodes: vec![
                BaseStaticNode {
                    id: "start".into(),
                    node_type: StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "v1".into(),
                    node_type: StaticNodeType::Variable,
                    name: Some("v1".into()),
                    description: None,
                    config: Some(serde_json::json!({
                        "variable_name": "final",
                        "expression": "${input.greeting}",
                    })),
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "end".into(),
                    node_type: StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                wf_types::workflow::Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "v1".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e2".into(),
                    source_node_id: "v1".into(),
                    target_node_id: "end".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
            ],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            hooks: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    /// A workflow with two intermediate variable nodes so a run can be
    /// stopped partway (start -> v1 -> v2 -> end) and resumed from a
    /// mid-execution checkpoint.
    fn make_multi_step_definition(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: None,
            r#type: None,
            version: Some("1.0.0".into()),
            nodes: vec![
                BaseStaticNode {
                    id: "start".into(),
                    node_type: StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "v1".into(),
                    node_type: StaticNodeType::Variable,
                    name: Some("v1".into()),
                    description: None,
                    config: Some(serde_json::json!({
                        "variable_name": "step1",
                        "expression": "${input.greeting}",
                    })),
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "v2".into(),
                    node_type: StaticNodeType::Variable,
                    name: Some("v2".into()),
                    description: None,
                    config: Some(serde_json::json!({
                        "variable_name": "final",
                        "expression": "${variables.step1}-done",
                    })),
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "end".into(),
                    node_type: StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                wf_types::workflow::Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "v1".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e2".into(),
                    source_node_id: "v1".into(),
                    target_node_id: "v2".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e3".into(),
                    source_node_id: "v2".into(),
                    target_node_id: "end".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
            ],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            hooks: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    #[test]
    fn converts_definition_to_graph() {
        let graph = definition_to_graph(&make_definition("wf-graph"));
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.start_node_id.as_deref(), Some("start"));
        assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
        assert_eq!(
            graph.nodes[1]
                .inner
                .get("variable_name")
                .and_then(|v| v.as_str()),
            Some("final")
        );
        assert_eq!(graph.nodes[1].node_type, "VARIABLE");
    }

    #[test]
    fn derives_boundaries_from_node_types_not_positions() {
        let mut definition = make_definition("wf-order");
        definition.nodes.reverse();
        let graph = definition_to_graph(&definition);
        assert_eq!(graph.start_node_id.as_deref(), Some("start"));
        assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
    }

    #[tokio::test]
    async fn executes_workflow_and_queries_status() {
        let ctx = make_ctx();
        let definition = make_definition("wf-exec-1");
        ctx.storage.workflow.save(&definition).await.unwrap();

        let output = execute(
            &ctx,
            ExecuteWorkflowParams {
                workflow_id: "wf-exec-1".into(),
                input: Some(serde_json::json!({"greeting": "hello"})),
                options: None,
            },
        )
        .await
        .expect("workflow should complete");
        assert!(!output.execution_id.is_empty());
        assert_eq!(output.result, serde_json::json!({"greeting": "hello"}));

        let status = status(&ctx, &output.execution_id.to_string())
            .await
            .expect("status query");
        assert_eq!(status, wf_types::ExecutionStatus::Completed);

        // The execution record is persisted with the full snapshot
        // (status / variables / node results / graph / timestamps).
        use wf_storage::adapter::base::BaseStorageAdapter;
        let executions = ctx.storage.workflow_execution.list(None).await.unwrap();
        assert_eq!(executions.len(), 1, "workflow execution must be persisted");
        let record = executions.into_iter().next().unwrap();
        assert_eq!(record.id, output.execution_id);
        assert_eq!(record.workflow_id, wf_types::Id::from("wf-exec-1"));
        assert_eq!(record.status, wf_types::ExecutionStatus::Completed);
        assert!(record.graph.is_some(), "graph must be captured");
        assert_eq!(
            record.output,
            Some(serde_json::json!({"greeting": "hello"}))
        );
        let variables = record.variables.expect("variables captured");
        assert!(
            variables.iter().any(|v| v.name == "final"),
            "workflow variables must be captured"
        );
        let node_results = record.node_results.expect("node results captured");
        assert!(
            node_results.iter().any(|r| r.node_id == "v1"),
            "node results must be captured"
        );
    }

    /// After a real `execute`, the persisted record is
    /// readable through a fresh context (empty live registries), so the
    /// persisted branches of the history / execution-state queries return real
    /// data.
    #[tokio::test]
    async fn persisted_execution_readable_after_restart() {
        use wf_core::EventBus;
        use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
        use wf_metrics::MetricsRegistry;

        let storage = Arc::new(StorageContext::new_memory());
        let definition = make_definition("wf-persist");
        storage.workflow.save(&definition).await.unwrap();

        let mock = Arc::new(MockLlmClient::new());
        mock.script(LlmResponseSpec::text("ok"));
        let gateway = Arc::new(LlmGateway::new());
        gateway.register_mock("mock", mock);

        let mut ctx1 = ApiContext::from_runtime_parts(
            storage.clone(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
            Arc::new(EventBus::new(64)),
            gateway.clone(),
            Arc::new(wf_tools::create_default_tool_registry()),
            Some(Arc::new(MetricsRegistry::new())),
        );
        ctx1 =
            ctx1.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::new_memory()));

        let ctx1 = Arc::new(ctx1);
        let output = execute(
            &ctx1,
            ExecuteWorkflowParams {
                workflow_id: "wf-persist".into(),
                input: Some(serde_json::json!({"greeting": "persist"})),
                options: None,
            },
        )
        .await
        .expect("workflow completes");
        let execution_id = output.execution_id.to_string();

        // A fresh context over the same storage has empty live registries: the
        // execution-state query must fall back to the persisted record.
        let ctx2 = Arc::new(ApiContext::from_runtime_parts(
            storage,
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
            Arc::new(EventBus::new(64)),
            gateway,
            Arc::new(wf_tools::create_default_tool_registry()),
            None,
        ));

        use crate::workflow::execution_state::workflow_execution_get_state;
        let view = workflow_execution_get_state(&ctx2, &execution_id)
            .await
            .expect("persisted state query");
        assert_eq!(view.source, "persisted");
        assert_eq!(view.status, wf_types::ExecutionStatus::Completed);
        assert!(
            view.variables.contains_key("final"),
            "workflow variables must round-trip through the persisted record"
        );

        // The execution is also visible to `list_executions` and `search`.
        let listed = crate::workflow::list_executions(&ctx2, None).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, output.execution_id);

        use crate::analysis::search::{SearchOptions, SearchResourceType};
        let hits = crate::analysis::search::search(
            &ctx2,
            "wf-persist",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Execution]),
                ..Default::default()
            },
        )
        .await
        .expect("search executions");
        assert!(
            hits.items.iter().any(|h| h.id == execution_id),
            "persisted execution must be searchable"
        );
    }

    #[tokio::test]
    async fn rejects_unknown_workflow() {
        let ctx = make_ctx();
        let err = execute(
            &ctx,
            ExecuteWorkflowParams {
                workflow_id: "missing".into(),
                input: None,
                options: None,
            },
        )
        .await
        .expect_err("unknown workflow must fail");
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn checkpoint_create_and_restore_roundtrip() {
        let ctx = make_ctx();
        let definition = make_definition("wf-cp-1");
        ctx.storage.workflow.save(&definition).await.unwrap();

        let output = execute(
            &ctx,
            ExecuteWorkflowParams {
                workflow_id: "wf-cp-1".into(),
                input: Some(serde_json::json!({"greeting": "cp"})),
                options: None,
            },
        )
        .await
        .expect("workflow completes");

        let id = output.execution_id.to_string();
        let checkpoint_id = create_checkpoint(&ctx, &id)
            .await
            .expect("create checkpoint");

        let restored = restore_checkpoint(&ctx, &checkpoint_id)
            .await
            .expect("restore checkpoint");
        assert_eq!(restored.execution_id, id);
        assert_eq!(restored.checkpoint_id, checkpoint_id);
        assert_eq!(restored.status, "Completed");
        // Node results captured in the checkpoint snapshot round-trip.
        let node_results = restored.node_results.expect("node results captured");
        assert!(node_results.contains_key("v1"));

        // Checkpoints are persisted onto the shared checkpoint store. With
        // checkpoints enabled by default the execution itself creates a
        // start/node/end snapshot chain, and the manual checkpoint is
        // appended on top.
        use wf_storage::domain::store::Store;
        let listed = ctx.checkpoint_store.list(None).await.unwrap();
        assert!(
            !listed.is_empty(),
            "execution checkpoints must be persisted on the shared store"
        );
    }

    /// A checkpoint captured mid-execution restores into a
    /// runnable entity that resumes from the breakpoint and produces the same
    /// final result as a full uninterrupted run.
    #[tokio::test]
    async fn restore_checkpoint_then_resume_continues_from_breakpoint() {
        let ctx = make_ctx();
        let definition = make_multi_step_definition("wf-cp-break");
        ctx.storage.workflow.save(&definition).await.unwrap();

        let full = execute(
            &ctx,
            ExecuteWorkflowParams {
                workflow_id: "wf-cp-break".into(),
                input: Some(serde_json::json!({"greeting": "hi"})),
                options: None,
            },
        )
        .await
        .expect("full run completes");

        // Stop partway (after start + v1) so the live entity holds a genuine
        // mid-execution state: v1's result is recorded, v2/end are not.
        let mut partial_options = default_options();
        partial_options.max_steps = Some(2);
        let partial = execute(
            &ctx,
            ExecuteWorkflowParams {
                workflow_id: "wf-cp-break".into(),
                input: Some(serde_json::json!({"greeting": "hi"})),
                options: Some(partial_options),
            },
        )
        .await
        .expect("partial run completes");
        let execution_id = partial.execution_id.to_string();
        let entity = ctx.workflow_execution(&execution_id).expect("live entity");
        let completed = entity.state.read().await.completed_nodes().to_vec();
        assert!(
            completed.contains(&"v1".to_string()) && !completed.contains(&"v2".to_string()),
            "partial run must stop after v1, got {completed:?}"
        );

        let checkpoint_id = create_checkpoint(&ctx, &execution_id)
            .await
            .expect("create mid-run checkpoint");

        let restored = restore_checkpoint(&ctx, &checkpoint_id)
            .await
            .expect("restore checkpoint");
        assert_eq!(restored.execution_id, execution_id);
        // The restored snapshot carries v1's result and the breakpoint node.
        let node_results = restored.node_results.expect("node results captured");
        assert!(node_results.contains_key("v1"));
        assert!(
            !node_results.contains_key("v2"),
            "v2 must not have run before the breakpoint"
        );
        assert!(
            restored.variables.contains_key("step1"),
            "step1 variable must be restored"
        );

        // Restore-and-resume drives the restored entity to completion and the
        // final output matches the uninterrupted full run.
        let resumed = restore_and_resume(&ctx, &checkpoint_id)
            .await
            .expect("restore and resume");
        assert_eq!(resumed.result, full.result);
        assert_eq!(resumed.execution_id.to_string(), execution_id);
    }

    #[tokio::test]
    async fn resume_returns_resumed_execution_result() {
        let ctx = make_ctx();
        let definition = make_definition("wf-resume-1");
        ctx.storage.workflow.save(&definition).await.unwrap();

        let output = execute(
            &ctx,
            ExecuteWorkflowParams {
                workflow_id: "wf-resume-1".into(),
                input: Some(serde_json::json!({"greeting": "r"})),
                options: None,
            },
        )
        .await
        .expect("workflow completes");

        let resumed = resume(&ctx, &output.execution_id.to_string())
            .await
            .expect("resume completed execution");
        assert_eq!(resumed.result, serde_json::json!({"greeting": "r"}));
    }

    #[tokio::test]
    async fn short_timeout_maps_to_timeout_error() {
        // The `with_timeout` primitive is what `execute`/`run` wrap their
        // futures with; a short deadline over a slow future must map onto
        // `ApiError::Timeout`.
        let err = crate::infra::error::with_timeout(Duration::from_millis(10), async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok::<_, ApiError>(WorkflowOutput {
                execution_id: wf_types::Id::from("x".to_string()),
                result: Value::Null,
            })
        })
        .await
        .expect_err("short deadline must elapse");
        assert!(matches!(err, ApiError::Timeout(_)));
    }

    #[tokio::test]
    async fn execution_summaries_project_persisted_records() {
        let ctx = make_ctx();
        let definition = make_definition("wf-sum-1");
        ctx.storage.workflow.save(&definition).await.unwrap();
        let output = execute(
            &ctx,
            ExecuteWorkflowParams {
                workflow_id: "wf-sum-1".into(),
                input: Some(serde_json::json!({"greeting": "hi"})),
                options: None,
            },
        )
        .await
        .expect("workflow completes");

        let summaries = execution_summaries(&ctx, None).await.unwrap();
        let execution = summaries
            .iter()
            .find(|s| s.id.as_str() == output.execution_id.as_str())
            .expect("execution present");
        assert_eq!(execution.workflow_id, "wf-sum-1");
        assert!(matches!(
            execution.status,
            wf_types::ExecutionStatus::Completed
        ));
        assert!(execution.elapsed_ms.is_some());
        assert_eq!(execution.error_count, 0);

        let filtered = execution_summaries(
            &ctx,
            Some(
                wf_storage::adapter::execution::WorkflowExecutionListOptions {
                    workflow_id_filter: Some("wf-sum-1".into()),
                    ..Default::default()
                },
            ),
        )
        .await
        .unwrap();
        assert_eq!(filtered.len(), 1);
    }
}

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
use wf_checkpoint::coordinator::CheckpointCoordinator;
use checkpoint_state::WorkflowCheckpointStateManager;
use wf_core::registry::MutableRegistry;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::CheckpointVariableState;
use wf_types::execution::ExecutionHierarchy;
use wf_types::workflow_execution::WorkflowExecutionOptions;
use wf_workflow::checkpoint::{NodeCheckpointStrategy, WorkflowCheckpointIntegration};
use wf_workflow::entity::WorkflowExecutionEntity;
use wf_workflow::WorkflowCoordinator;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiError;

use super::lifecycle::{default_options, execution_options, live_entity, EXECUTION_OPTIONS_VAR};

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
        .create_manual_checkpoint(execution_id, snapshot)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint creation failed: {e}")))?;
    Ok(checkpoint_id)
}

/// Restore execution state from a checkpoint.
///
/// The checkpoint's `current_node_id` / `node_results` / `variable_state` are
/// backfilled into a fresh live entity (registered under the checkpoint's
/// execution id) together with the captured execution options, so the
/// restored execution continues through the standard
/// [`super::lifecycle::resume`] coordinator path.
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

    // Build a fresh live entity backfilled from the restored snapshot. The
    // hierarchy manager is rebuilt from the snapshot in one place so the
    // restored parent, ancestors, depth and root stay consistent.
    let mut entity =
        WorkflowExecutionEntity::new(snapshot.execution_id.clone(), workflow_id.clone());
    if let Some(hierarchy) = snapshot.hierarchy.as_ref() {
        let manager = wf_core::hierarchy::manager::ExecutionHierarchyManager::restore(
            entity.id().clone(),
            wf_types::execution::ExecutionType::Workflow,
            hierarchy,
            wf_types::execution::ExecutionType::Workflow,
        );
        entity = entity.with_hierarchy_manager(manager);
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
    // Carry the pending error-branch suspend record so a resumed run can
    // rebuild the isolated branch scope from typed state.
    if let Some(suspend) = snapshot.error_suspend.clone() {
        entity.state.write().await.set_error_suspend(Some(suspend));
    }
    // Restore the captured execution options so `resume` rebuilds the same
    // input/options. Step budgets are consumed by the original run and are not
    // re-applied; the wall-clock budget is reduced to whatever remains after
    // the time the original run had already spent at checkpoint time, so a
    // continuation can never outlive the original `max_execution_time`.
    let mut continuation_options = options;
    continuation_options.max_steps = None;
    continuation_options.max_execution_time = match continuation_options.max_execution_time {
        Some(budget) if budget > 0 => {
            let executed_ms = snapshot
                .execution_config
                .as_ref()
                .and_then(|config| config.get("executed_ms"))
                .and_then(|value| value.as_i64())
                .unwrap_or(0)
                .max(0) as u64;
            let remaining = budget.saturating_sub(executed_ms);
            if remaining == 0 {
                return Err(ApiError::execution(format!(
                    "execution {execution_id} already exhausted its {budget}ms wall-clock \
                         budget before the checkpoint; refusing to resume with no remaining budget"
                )));
            }
            Some(remaining)
        }
        // Absent or 0 means unlimited: the continuation stays unbudgeted.
        other => other,
    };
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
/// Equivalent to [`restore_checkpoint`] followed by
/// [`super::lifecycle::resume`] on the restored execution.
pub async fn restore_and_resume(
    ctx: &ApiContext,
    checkpoint_id: &str,
) -> crate::infra::error::ApiResult<wf_tools::callback::WorkflowOutput> {
    let restored = restore_checkpoint(ctx, checkpoint_id).await?;
    super::lifecycle::resume(ctx, &restored.execution_id).await
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

/// Extract the named message contexts (active view + archived history) from a
/// variable map into the first-class checkpoint domain, deduplicated by
/// message id. Mirrors the engine's snapshot builder so the API-built
/// snapshots stay consistent with the live ones.
fn message_contexts_from_vars(
    variables: &HashMap<String, serde_json::Value>,
) -> Option<HashMap<String, wf_types::checkpoint::workflow::MessageContextSnapshot>> {
    use wf_workflow::message_context::{CONTEXT_HISTORY_PREFIX, CONTEXT_PREFIX, LEDGER_PREFIX};

    let mut contexts: HashMap<String, wf_types::checkpoint::workflow::MessageContextSnapshot> =
        HashMap::new();
    for (key, value) in variables.iter() {
        if let Some(context_id) = key.strip_prefix(CONTEXT_PREFIX) {
            if let Ok(messages) =
                serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone())
            {
                let version = variables
                    .get(LEDGER_PREFIX)
                    .and_then(|v| {
                        serde_json::from_value::<wf_types::llm::TokenLedger>(v.clone()).ok()
                    })
                    .map(|l| l.version(context_id))
                    .unwrap_or(0);
                contexts
                    .entry(context_id.to_string())
                    .or_insert_with(|| wf_types::checkpoint::workflow::MessageContextSnapshot {
                        messages: Vec::new(),
                        version,
                    })
                    .messages
                    .extend(messages);
            }
        }
    }
    for (key, value) in variables.iter() {
        if let Some(context_id) = key.strip_prefix(CONTEXT_HISTORY_PREFIX) {
            if let Ok(messages) =
                serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone())
            {
                let entry = contexts.entry(context_id.to_string()).or_insert_with(|| {
                    wf_types::checkpoint::workflow::MessageContextSnapshot {
                        messages: Vec::new(),
                        version: 0,
                    }
                });
                let known: std::collections::HashSet<String> =
                    entry.messages.iter().map(|m| m.id.clone()).collect();
                for message in messages {
                    if !known.contains(&message.id) {
                        entry.messages.push(message);
                    }
                }
            }
        }
    }
    (!contexts.is_empty()).then_some(contexts)
}

/// Build a resume snapshot from the entity's current completion state:
/// node results seed the coordinator's outputs and completed set, the
/// current node restarts execution from there.
pub(crate) async fn entity_resume_snapshot(
    ctx: &ApiContext,
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
    let message_contexts = message_contexts_from_vars(&variables);
    let hierarchy = build_hierarchy(entity).await;
    let fork_join_aggregation_state = build_fork_aggregation_state(ctx, entity);
    WorkflowExecutionStateSnapshot {
        execution_id: entity.id().to_string(),
        status: state.status().as_str().to_string(),
        current_node_id: state.current_node_id().map(String::from),
        node_results: Some(node_results),
        node_execution_records,
        variable_state: CheckpointVariableState { variables },
        message_contexts,
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
        hierarchy,
        execution_config: None,
        fork_join_aggregation_state,
        hook_execution_context: None,
        error_suspend: state.error_suspend().cloned(),
    }
}

/// Build a full checkpoint snapshot from the entity's live state.
///
/// Enriched beyond the resume-view snapshot so a cross-process restore
/// reconstructs a runnable execution: the captured execution options
/// (input + options, used to rebuild the `ExecutorContext`), the execution
/// hierarchy (the parent/root links a child records about itself) and the
/// recorded error records are all persisted. `fork_join_context` is not
/// tracked on the entity; Fork aggregation state is derived from the manager
/// fork children against the live execution registry, so it is recomputed on
/// every checkpoint rather than restored.
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
    let message_contexts = message_contexts_from_vars(&variables);

    WorkflowExecutionStateSnapshot {
        execution_id: entity.id().to_string(),
        status: state.status().as_str().to_string(),
        current_node_id: state.current_node_id().map(String::from),
        node_results: Some(node_results),
        node_execution_records,
        variable_state: CheckpointVariableState { variables },
        message_contexts,
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
            // Freeze the wall-clock time the original run had already spent
            // when this checkpoint was taken, so a restore can grant the
            // continuation only the remaining budget.
            "executed_ms": (wf_common::now() - state.start_time()).max(0),
        })),
        fork_join_aggregation_state: build_fork_aggregation_state(ctx, entity),
        hook_execution_context: None,
        error_suspend: state.error_suspend().cloned(),
    }
}

/// Build the execution hierarchy captured at checkpoint time: read directly
/// from the entity hierarchy manager so fork provenance, parent type and the
/// ancestor chain survive the snapshot instead of being refabricated.
/// Forward links only — the child side of the tree is discovered by querying
/// the child records, never from a list cached in this snapshot.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn build_hierarchy(
    entity: &WorkflowExecutionEntity,
) -> Option<ExecutionHierarchy> {
    use wf_execution_shared::types::execution_entity::ExecutionEntity;
    let manager = entity.hierarchy_manager();
    let parent = manager.parent();
    let ancestors = entity.get_ancestors();
    if parent.is_none() && ancestors.is_empty() && manager.fork_path().is_none() {
        return None;
    }
    Some(ExecutionHierarchy::new(
        entity.workflow_id().clone(),
        entity.id().clone(),
        ancestors,
        parent.as_ref().map(|p| p.parent_type.clone()),
        Some(manager.root_execution_type()),
        manager.fork_path(),
    ))
}

/// Build the fork aggregation record from the manager's fork children,
/// resolving each branch path status from the live execution registry.
/// A branch with no live handle reads as pending, which is also the state a
/// branch that has not started yet is in.
fn build_fork_aggregation_state(
    ctx: &ApiContext,
    entity: &WorkflowExecutionEntity,
) -> Option<serde_json::Value> {
    use wf_execution_shared::fork::{fork_aggregation_state, ForkChildStatus};
    use wf_execution_shared::types::execution_entity::{ExecutionEntity, ExecutionStatus};
    let children = entity.hierarchy_manager().children();
    fork_aggregation_state(&children, |child_id| {
        match ctx
            .execution_instance(child_id.as_str())
            .map(|handle| handle.status())
        {
            Some(ExecutionStatus::Completed) => ForkChildStatus::Completed,
            Some(
                ExecutionStatus::Failed
                | ExecutionStatus::Cancelled
                | ExecutionStatus::Stopped
                | ExecutionStatus::Timeout,
            ) => ForkChildStatus::Failed,
            _ => ForkChildStatus::Pending,
        }
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
/// execution that has checkpoints enabled. Roots snapshot after every
/// completed node; sub-executions use a sparser cadence to bound write
/// amplification. Start/end snapshots are unconditional (not strategy-gated).
fn checkpoint_integration(ctx: &ApiContext, depth: u32) -> WorkflowCheckpointIntegration {
    let mut integration = WorkflowCheckpointIntegration::new(
        ctx.checkpoint_store.clone(),
        NodeCheckpointStrategy::for_depth(depth),
    )
    .with_core_event_bus(ctx.event_bus.clone());
    if let Some(manager) = ctx.file_checkpoint_manager() {
        integration = integration.with_file_checkpoint_manager(manager.clone());
    }
    integration
}

/// Attach the checkpoint integration when `enable_checkpoints` is not
/// explicitly disabled.
pub(crate) fn attach_checkpoints(
    coordinator: WorkflowCoordinator,
    ctx: &ApiContext,
    checkpoints_enabled: bool,
    depth: u32,
) -> WorkflowCoordinator {
    if checkpoints_enabled {
        coordinator.with_checkpoint(checkpoint_integration(ctx, depth))
    } else {
        coordinator
    }
}

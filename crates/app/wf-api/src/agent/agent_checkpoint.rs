//! Agent loop checkpoint management through the unified coordinator store.
//!
//! Manual checkpoints share the checkpoint store and chain logic with the
//! engine-created ones: the coordinator decides the storage type, links the
//! chain and persists the blob, so listing, resume and audit observe a single
//! checkpoint history per agent loop. Restore replays the snapshot onto the
//! live entity when the loop is still registered.

use std::collections::BTreeMap;

use serde::Serialize;

use wf_checkpoint::coordinator::agent::AgentCheckpointCoordinator;
use wf_checkpoint::coordinator::CheckpointCoordinator;
use wf_checkpoint::delta::CheckpointLoader;
use wf_checkpoint::state::agent::AgentCheckpointStateManager;
use wf_checkpoint::state::CheckpointStateManager;
use wf_execution_shared::types::state_manager::StateManager;
use wf_types::checkpoint::base::{CheckpointStatus, CheckpointType};
use wf_types::checkpoint::CheckpointTiming;
use wf_types::Checkpoint;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

/// Agent loop checkpoint statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentCheckpointStatistics {
    pub total: usize,
    pub by_type: BTreeMap<String, usize>,
    pub active: usize,
    /// Average blob size of the retained checkpoints (bytes).
    pub avg_blob_size: Option<u64>,
}

fn state_manager(ctx: &ApiContext) -> AgentCheckpointStateManager {
    AgentCheckpointStateManager::new(ctx.checkpoint_store.clone())
}

fn coordinator(ctx: &ApiContext) -> AgentCheckpointCoordinator {
    let mut coordinator = AgentCheckpointCoordinator::new(state_manager(ctx));
    if let Some(manager) = ctx.file_checkpoint_manager() {
        coordinator = coordinator.with_file_checkpoint_manager(manager.clone());
    }
    coordinator
}

/// Create a manual checkpoint for an agent loop.
///
/// A live loop is snapshotted through the engine integration (full runtime
/// state including the conversation); otherwise an empty snapshot seeds the
/// chain. The coordinator decides the storage type and links the chain, so
/// the caller only supplies an optional description recorded in the
/// checkpoint metadata.
pub async fn create(
    ctx: &ApiContext,
    agent_loop_id: &str,
    description: Option<String>,
) -> ApiResult<Checkpoint> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let mut integration =
            wf_agent::checkpoint::AgentCheckpointIntegration::new(ctx.checkpoint_store.clone());
        if let Some(manager) = ctx.file_checkpoint_manager() {
            integration = integration.with_file_checkpoint_manager(manager.clone());
        }
        integration
            .create_checkpoint(&entity, CheckpointTiming::Manual, description)
            .await
            .map_err(|e| ApiError::execution(format!("checkpoint creation failed: {e}")))?;
    } else {
        let coordinator = coordinator(ctx);
        if !coordinator.manual_allowed() {
            return Err(ApiError::execution(
                "manual checkpoint rejected: checkpointing is disabled",
            ));
        }
        let mut prepare = coordinator
            .prepare(agent_loop_id, CheckpointTiming::Manual)
            .await
            .map_err(|e| ApiError::execution(format!("checkpoint creation failed: {e}")))?;
        if let Some(text) = description {
            prepare
                .metadata
                .get_or_insert_default()
                .insert("description".to_string(), serde_json::json!(text));
        }
        let checkpoint = coordinator
            .build(prepare, empty_snapshot(agent_loop_id))
            .await
            .map_err(|e| ApiError::execution(format!("checkpoint creation failed: {e}")))?;
        coordinator
            .validate_checkpoint(&checkpoint)
            .await
            .map_err(|e| ApiError::execution(format!("checkpoint creation failed: {e}")))?;
        coordinator
            .persist(&checkpoint, agent_loop_id)
            .await
            .map_err(|e| ApiError::execution(format!("checkpoint creation failed: {e}")))?;
        let _ = coordinator
            .save_file_snapshot(&checkpoint.id, agent_loop_id)
            .await;
    }
    state_manager(ctx)
        .get_latest(agent_loop_id)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint lookup failed: {e}")))?
        .ok_or_else(|| not_found("checkpoint", agent_loop_id))
}

/// Seed snapshot for an agent loop with no live state to read.
fn empty_snapshot(agent_loop_id: &str) -> wf_types::checkpoint::agent::AgentStateSnapshot {
    wf_types::checkpoint::agent::AgentStateSnapshot {
        agent_loop_id: agent_loop_id.to_string(),
        status: "running".into(),
        current_iteration: 0,
        tool_call_count: 0,
        conversation_snapshot: None,
        conversation_view: None,
        message_seq_start: None,
        message_seq_end: None,
        message_next_seq: None,
        conversation_ledger: None,
        conversation_tracker: None,
        tool_call_history: None,
        is_streaming: None,
        variable_snapshots: None,
        error: None,
        started_at: Some(wf_common::now()),
        completed_at: None,
        error_records: None,
        interruption_records: None,
        event_records: None,
        iteration_history: None,
        current_iteration_record: None,
        stream_message: None,
        pending_tool_call_ids: None,
        trigger_state: None,
        hierarchy: None,
        messages: None,
        tool_discovery_state: None,
    }
}

/// Restore an agent loop from a checkpoint: verifies ownership, restores the
/// full state through the coordinator (delta chains resolved) and replays it
/// onto the live entity when the loop is still registered.
pub async fn restore(
    ctx: &ApiContext,
    agent_loop_id: &str,
    checkpoint_id: &str,
) -> ApiResult<Checkpoint> {
    let coordinator = coordinator(ctx);
    let restored = coordinator.restore(checkpoint_id).await.map_err(|e| {
        if matches!(e, wf_checkpoint::CheckpointError::NotFound { .. }) {
            not_found("checkpoint", checkpoint_id)
        } else {
            ApiError::execution(format!("checkpoint restore failed: {e}"))
        }
    })?;
    if restored.snapshot.agent_loop_id != agent_loop_id {
        return Err(ApiError::Validation(format!(
            "checkpoint {checkpoint_id} does not belong to agent loop {agent_loop_id}"
        )));
    }
    let snapshot = restored.snapshot;

    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let mut state = entity.state.write().await;
        state
            .restore_from_snapshot(wf_agent::state::AgentLoopStateSnapshot {
                status: parse_checkpoint_status(&snapshot.status),
                current_iteration: snapshot.current_iteration,
                tool_call_count: snapshot.tool_call_count,
                iteration_history: snapshot
                    .iteration_history
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|value| serde_json::from_value(value).ok())
                    .collect(),
                start_time: snapshot.started_at.unwrap_or(wf_common::now()),
                end_time: snapshot.completed_at,
                error: snapshot.error,
                error_records: Vec::new(),
                variable_snapshots: snapshot
                    .variable_snapshots
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, var)| (name, var.value))
                    .collect(),
                tool_discovery: snapshot
                    .tool_discovery_state
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default(),
                pending_tool_calls: snapshot
                    .pending_tool_call_ids
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                completed_tool_results: std::collections::HashMap::new(),
                interruption_records: snapshot.interruption_records.unwrap_or_default(),
                event_records: snapshot.event_records.unwrap_or_default(),
                locked_tool_call_protocol: None,
                timeout_count: 0,
            })
            .await
            .map_err(|e| ApiError::execution(format!("state restore failed: {e}")))?;
    } else {
        // The checkpoint is already persisted; a loop that is not currently
        // running has no live state to replay, so the restore is idempotent:
        // it does not fail for an absent loop.
        tracing::warn!(
            target: "wf_api",
            agent_loop_id,
            checkpoint_id,
            "restore: agent loop is not running; checkpoint left as-is"
        );
    }

    state_manager(ctx)
        .load_metadata(checkpoint_id)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint lookup failed: {e}")))?
        .ok_or_else(|| not_found("checkpoint", checkpoint_id))
}

/// Checkpoints of one agent loop, newest first.
pub async fn list(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<Checkpoint>> {
    let mut checkpoints = state_manager(ctx)
        .list_by_entity(agent_loop_id)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint lookup failed: {e}")))?;
    // Newest first; tie-break on id so same-millisecond checkpoints keep a
    // stable, deterministic order regardless of the store's iteration order.
    checkpoints.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.id.cmp(&a.id)));
    Ok(checkpoints)
}

/// Checkpoint chains of one agent loop: consecutive checkpoints grouped by
/// their chain root, each chain ordered oldest first.
pub async fn chain(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<Vec<Checkpoint>>> {
    let checkpoints = list(ctx, agent_loop_id).await?;
    let mut chains: BTreeMap<String, Vec<Checkpoint>> = BTreeMap::new();
    for checkpoint in checkpoints {
        let root = checkpoint
            .chain_root_id
            .clone()
            .unwrap_or_else(|| checkpoint.id.to_string());
        chains.entry(root).or_default().push(checkpoint);
    }
    let mut result: Vec<Vec<Checkpoint>> = chains.into_values().collect();
    for chain in &mut result {
        chain.sort_by_key(|c| c.chain_position.unwrap_or(0));
    }
    Ok(result)
}

/// Delete all checkpoints of one agent loop; returns the number removed.
pub async fn delete_for(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<u64> {
    let manager = state_manager(ctx);
    let checkpoints = manager
        .list_by_entity(agent_loop_id)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint lookup failed: {e}")))?;
    let mut removed = 0u64;
    for checkpoint in checkpoints {
        let deleted = manager
            .delete(&checkpoint.id)
            .await
            .map_err(|e| ApiError::execution(format!("checkpoint delete failed: {e}")))?;
        if deleted {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Statistics over the checkpoints of one agent loop (or all agent loops
/// when the id is `None`).
pub async fn statistics(
    ctx: &ApiContext,
    agent_loop_id: Option<&str>,
) -> ApiResult<AgentCheckpointStatistics> {
    let all = match agent_loop_id {
        Some(id) => list(ctx, id).await?,
        None => global_checkpoints(ctx).await?,
    };
    let mut stats = AgentCheckpointStatistics {
        total: all.len(),
        ..AgentCheckpointStatistics::default()
    };
    let mut blob_total = 0u64;
    for checkpoint in all {
        let type_name = match checkpoint.checkpoint_type {
            CheckpointType::Full => "full",
            CheckpointType::Delta => "delta",
        };
        *stats.by_type.entry(type_name.to_string()).or_insert(0) += 1;
        if matches!(
            checkpoint.status,
            CheckpointStatus::Active | CheckpointStatus::Completed
        ) {
            stats.active += 1;
        }
        if let Some(size) = checkpoint.blob_size {
            blob_total += size;
        }
    }
    if stats.total > 0 {
        stats.avg_blob_size = Some(blob_total / stats.total as u64);
    }
    Ok(stats)
}

/// Every agent loop checkpoint in the shared store, oldest first.
async fn global_checkpoints(ctx: &ApiContext) -> ApiResult<Vec<Checkpoint>> {
    use wf_storage::domain::store::{QueryFilter, Store};

    let filter = QueryFilter::new().with_field("entityType", "agent_loop");
    let entries = ctx
        .checkpoint_store
        .list(Some(&filter))
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint lookup failed: {e}")))?;
    let mut checkpoints: Vec<Checkpoint> = entries
        .iter()
        .map(|(id, meta)| {
            let entity_id = meta
                .get("entityId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            wf_checkpoint::state::parse_storage_metadata(id, entity_id, meta)
        })
        .filter(|checkpoint| checkpoint.entity_type == "agent_loop")
        .collect();
    checkpoints.sort_by_key(|c| c.timestamp);
    Ok(checkpoints)
}

/// Parse a checkpoint status string onto the live execution status contract.
fn parse_checkpoint_status(
    status: &str,
) -> wf_execution_shared::types::execution_entity::ExecutionStatus {
    match status {
        "created" => wf_execution_shared::types::execution_entity::ExecutionStatus::Created,
        "running" => wf_execution_shared::types::execution_entity::ExecutionStatus::Running,
        "paused" => wf_execution_shared::types::execution_entity::ExecutionStatus::Paused,
        "stopped" => wf_execution_shared::types::execution_entity::ExecutionStatus::Stopped,
        "completed" => wf_execution_shared::types::execution_entity::ExecutionStatus::Completed,
        "failed" => wf_execution_shared::types::execution_entity::ExecutionStatus::Failed,
        "cancelled" => wf_execution_shared::types::execution_entity::ExecutionStatus::Cancelled,
        _ => wf_execution_shared::types::execution_entity::ExecutionStatus::Created,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        let mut ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        );
        ctx =
            ctx.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::new_memory()));
        Arc::new(ctx)
    }

    #[tokio::test]
    async fn create_list_and_statistics() {
        let ctx = make_ctx();

        let cp1 = create(&ctx, "loop-c", Some("initial".into()))
            .await
            .unwrap();
        assert_eq!(cp1.entity_type, "agent_loop");
        assert_eq!(cp1.entity_id, "loop-c");
        assert_eq!(cp1.checkpoint_type, CheckpointType::Full);
        assert_eq!(cp1.chain_position, Some(0));
        assert!(cp1.blob_size.is_some());

        // The coordinator links the chain and picks the storage type: the
        // second checkpoint is a delta of the first.
        let cp2 = create(&ctx, "loop-c", None).await.unwrap();
        assert_eq!(cp2.checkpoint_type, CheckpointType::Delta);
        assert_eq!(cp2.chain_position, Some(1));
        assert_eq!(cp2.previous_checkpoint_id.as_deref(), Some(cp1.id.as_str()));

        let list = list(&ctx, "loop-c").await.unwrap();
        assert_eq!(list.len(), 2);
        // Newest first.
        assert_eq!(list[0].id, cp2.id);

        let chains = chain(&ctx, "loop-c").await.unwrap();
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].len(), 2);

        let stats = statistics(&ctx, Some("loop-c")).await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 2);
        assert_eq!(stats.by_type.get("full"), Some(&1));
        assert_eq!(stats.by_type.get("delta"), Some(&1));
        assert!(stats.avg_blob_size.is_some());

        let global = statistics(&ctx, None).await.unwrap();
        assert_eq!(global.total, 2);
    }

    #[tokio::test]
    async fn restore_validates_ownership() {
        let ctx = make_ctx();
        let cp = create(&ctx, "loop-r", None).await.unwrap();

        // Restoring the loop's own checkpoint succeeds.
        let restored = restore(&ctx, "loop-r", &cp.id).await.unwrap();
        assert_eq!(restored.id, cp.id);

        // A checkpoint of another loop is rejected.
        let other = create(&ctx, "loop-other", None).await.unwrap();
        let err = restore(&ctx, "loop-r", &other.id).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));

        // Unknown checkpoint id is not found.
        let err = restore(&ctx, "loop-r", "missing").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn delete_for_removes_checkpoints() {
        let ctx = make_ctx();
        create(&ctx, "loop-d", None).await.unwrap();
        create(&ctx, "loop-d", None).await.unwrap();
        create(&ctx, "other", None).await.unwrap();

        let removed = delete_for(&ctx, "loop-d").await.unwrap();
        assert_eq!(removed, 2);
        assert!(list(&ctx, "loop-d").await.unwrap().is_empty());
        assert_eq!(list(&ctx, "other").await.unwrap().len(), 1);
    }
}

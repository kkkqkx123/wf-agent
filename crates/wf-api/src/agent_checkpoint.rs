use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use wf_checkpoint::serializer::{CheckpointCodec, CheckpointSerializer};
use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::checkpoint::CheckpointStorageAdapter;
use wf_storage::domain::store::Store;
use wf_types::checkpoint::base::{CheckpointStatus, CheckpointType};
use wf_types::Checkpoint;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// Agent loop checkpoint statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentCheckpointStatistics {
    pub total: usize,
    pub by_type: BTreeMap<String, usize>,
    pub active: usize,
    /// Average blob size of the retained checkpoints (bytes).
    pub avg_blob_size: Option<u64>,
}

/// Agent loop checkpoint management (TS `AgentLoopCheckpointResourceAPI`
/// counterpart).
///
/// Checkpoints are persisted through the shared checkpoint adapter scoped to
/// the agent loop entity (`entity_type = "agent_loop"`), with the content blob
/// stored in the context's checkpoint store. Restore is best-effort: the blob
/// is decoded into an agent state snapshot and replayed onto the live entity
/// when the loop is still registered.
pub struct AgentLoopCheckpointApi {
    ctx: Arc<ApiContext>,
}

impl AgentLoopCheckpointApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Create an agent loop checkpoint.
    pub async fn create(
        &self,
        agent_loop_id: &str,
        checkpoint_type: CheckpointType,
        tags: Option<Vec<String>>,
    ) -> ApiResult<Checkpoint> {
        let latest = self
            .ctx
            .storage
            .checkpoint
            .get_latest_by_entity(agent_loop_id, "checkpoint")
            .await?;
        let now = wf_common::now();

        let snapshot = self.build_snapshot(agent_loop_id, now).await?;

        let bytes = CheckpointSerializer::serialize(&snapshot, CheckpointCodec::Bincode)
            .map_err(|e| ApiError::Execution(format!("checkpoint serialize failed: {e}")))?;
        let blob_size = bytes.len() as u64;

        let checkpoint_id = wf_types::Id::from(wf_common::generate_id());
        let checkpoint = Checkpoint {
            id: checkpoint_id.clone(),
            entity_type: "agent_loop".into(),
            entity_id: agent_loop_id.to_string(),
            checkpoint_type,
            timestamp: now,
            status: CheckpointStatus::Active,
            previous_checkpoint_id: latest.as_ref().map(|c| c.id.to_string()),
            base_checkpoint_id: latest.as_ref().and_then(|c| c.base_checkpoint_id.clone()),
            chain_root_id: latest
                .as_ref()
                .and_then(|c| c.chain_root_id.clone())
                .or_else(|| latest.as_ref().map(|c| c.id.to_string()))
                .or_else(|| Some(checkpoint_id.to_string())),
            chain_position: Some(
                latest
                    .as_ref()
                    .map(|c| c.chain_position.unwrap_or(0) + 1)
                    .unwrap_or(0),
            ),
            blob_size: Some(blob_size),
            tags,
            custom_fields: None,
        };

        self.ctx.storage.checkpoint.save(&checkpoint).await?;
        self.ctx
            .checkpoint_store
            .save(&checkpoint.id, &bytes, &serde_json::Value::Null)
            .await?;
        Ok(checkpoint)
    }

    /// Encode the live agent state (or an empty snapshot when the loop is not
    /// registered) as the checkpoint blob.
    async fn build_snapshot(
        &self,
        agent_loop_id: &str,
        now: i64,
    ) -> ApiResult<wf_types::checkpoint::agent::AgentStateSnapshot> {
        let mut snapshot = wf_types::checkpoint::agent::AgentStateSnapshot {
            agent_loop_id: wf_types::Id::from(agent_loop_id.to_string()),
            status: "running".into(),
            current_iteration: 0,
            tool_call_count: 0,
            conversation_snapshot: None,
            tool_call_history: None,
            is_streaming: None,
            variable_snapshots: None,
            error: None,
            started_at: Some(now),
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
        };

        let Some(entity) = self.ctx.agent_loop(agent_loop_id) else {
            return Ok(snapshot);
        };
        let state = entity.state.read().await;
        let state_snapshot = state
            .create_snapshot()
            .await
            .map_err(|e| ApiError::Execution(format!("state snapshot failed: {e}")))?;
        let status: wf_types::ExecutionStatus = state_snapshot.status.clone().into();
        let variables = state_snapshot
            .variable_snapshots
            .into_iter()
            .map(|(name, value)| {
                (
                    name,
                    wf_types::checkpoint::agent::VariableSnapshot {
                        value,
                        r#type: "string".into(),
                        size: None,
                        updated: true,
                        source: "live".into(),
                    },
                )
            })
            .collect();
        snapshot.status = crate::agent_loop_registry::status_str(&status).to_string();
        snapshot.current_iteration = state_snapshot.current_iteration;
        snapshot.tool_call_count = state_snapshot.tool_call_count;
        snapshot.variable_snapshots = Some(variables);
        snapshot.error = state_snapshot.error.clone();
        snapshot.started_at = Some(state_snapshot.start_time);
        snapshot.completed_at = state_snapshot.end_time;
        Ok(snapshot)
    }

    /// Restore an agent loop from a checkpoint: verifies ownership, decodes
    /// the blob and replays the snapshot onto the live entity (best effort).
    pub async fn restore(&self, agent_loop_id: &str, checkpoint_id: &str) -> ApiResult<Checkpoint> {
        let checkpoint = self
            .ctx
            .storage
            .checkpoint
            .load(checkpoint_id)
            .await?
            .ok_or_else(|| ApiError::not_found("checkpoint", checkpoint_id))?;
        if checkpoint.entity_type != "agent_loop" || checkpoint.entity_id != agent_loop_id {
            return Err(ApiError::Validation(format!(
                "checkpoint {checkpoint_id} does not belong to agent loop {agent_loop_id}"
            )));
        }

        if let Some(entity) = self.ctx.agent_loop(agent_loop_id) {
            let Some((bytes, _)) = self.ctx.checkpoint_store.load(&checkpoint.id).await? else {
                return Err(ApiError::not_found("checkpoint_blob", checkpoint_id));
            };
            let snapshot = CheckpointSerializer::auto_deserialize::<
                wf_types::checkpoint::agent::AgentStateSnapshot,
            >(&bytes)
            .map_err(|e| ApiError::Execution(format!("checkpoint decode failed: {e}")))?;
            let mut state = entity.state.write().await;
            state
                .restore_from_snapshot(wf_agent::state::AgentLoopStateSnapshot {
                    status: parse_checkpoint_status(&snapshot.status),
                    current_iteration: snapshot.current_iteration,
                    tool_call_count: snapshot.tool_call_count,
                    iteration_history: Vec::new(),
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
                })
                .await
                .map_err(|e| ApiError::Execution(format!("state restore failed: {e}")))?;
        }

        Ok(checkpoint)
    }

    /// Checkpoints of one agent loop, newest first.
    pub async fn list(&self, agent_loop_id: &str) -> ApiResult<Vec<Checkpoint>> {
        let mut checkpoints = self
            .ctx
            .storage
            .checkpoint
            .list_by_entity(agent_loop_id, "checkpoint")
            .await?;
        // Newest first; tie-break on id so same-millisecond checkpoints keep a
        // stable, deterministic order regardless of the store's iteration order.
        checkpoints.sort_by(|a, b| {
            b.timestamp
                .cmp(&a.timestamp)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(checkpoints)
    }

    /// Checkpoint chains of one agent loop: consecutive checkpoints grouped by
    /// their chain root, each chain ordered oldest first.
    pub async fn chain(&self, agent_loop_id: &str) -> ApiResult<Vec<Vec<Checkpoint>>> {
        let checkpoints = self.list(agent_loop_id).await?;
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
    pub async fn delete_for(&self, agent_loop_id: &str) -> ApiResult<u64> {
        let removed = self
            .ctx
            .storage
            .checkpoint
            .delete_by_entity(agent_loop_id, "checkpoint")
            .await?;
        Ok(removed)
    }

    /// Statistics over the checkpoints of one agent loop (or all when the id
    /// is `None`).
    pub async fn statistics(&self, agent_loop_id: Option<&str>) -> ApiResult<AgentCheckpointStatistics> {
        let all = match agent_loop_id {
            Some(id) => self.list(id).await?,
            None => self.ctx.storage.checkpoint.list(None).await?,
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
            if checkpoint.status == CheckpointStatus::Active {
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
}

/// Parse a checkpoint status string onto the live execution status contract.
fn parse_checkpoint_status(status: &str) -> wf_execution_shared::types::execution_entity::ExecutionStatus {
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
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        let mut ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        );
        ctx = ctx.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::new_memory()));
        Arc::new(ctx)
    }

    #[tokio::test]
    async fn create_list_and_statistics() {
        let ctx = make_ctx();
        let api = AgentLoopCheckpointApi::new(ctx.clone());

        let cp1 = api
            .create("loop-c", CheckpointType::Full, Some(vec!["initial".into()]))
            .await
            .unwrap();
        assert_eq!(cp1.entity_type, "agent_loop");
        assert_eq!(cp1.entity_id, "loop-c");
        assert_eq!(cp1.chain_position, Some(0));
        assert!(cp1.blob_size.is_some());

        let cp2 = api
            .create("loop-c", CheckpointType::Delta, None)
            .await
            .unwrap();
        assert_eq!(cp2.chain_position, Some(1));
        assert_eq!(cp2.previous_checkpoint_id.as_deref(), Some(cp1.id.as_str()));

        let list = api.list("loop-c").await.unwrap();
        assert_eq!(list.len(), 2);
        // Newest first.
        assert_eq!(list[0].id, cp2.id);

        let chains = api.chain("loop-c").await.unwrap();
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].len(), 2);

        let stats = api.statistics(Some("loop-c")).await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 2);
        assert_eq!(stats.by_type.get("full"), Some(&1));
        assert_eq!(stats.by_type.get("delta"), Some(&1));
        assert!(stats.avg_blob_size.is_some());
    }

    #[tokio::test]
    async fn restore_validates_ownership() {
        let ctx = make_ctx();
        let api = AgentLoopCheckpointApi::new(ctx.clone());
        let cp = api.create("loop-r", CheckpointType::Full, None).await.unwrap();

        // Restoring the loop's own checkpoint succeeds.
        let restored = api.restore("loop-r", &cp.id).await.unwrap();
        assert_eq!(restored.id, cp.id);

        // A checkpoint of another loop is rejected.
        let other = api.create("loop-other", CheckpointType::Full, None).await.unwrap();
        let err = api.restore("loop-r", &other.id).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));

        // Unknown checkpoint id is not found.
        let err = api.restore("loop-r", "missing").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn delete_for_removes_checkpoints() {
        let ctx = make_ctx();
        let api = AgentLoopCheckpointApi::new(ctx.clone());
        api.create("loop-d", CheckpointType::Full, None).await.unwrap();
        api.create("loop-d", CheckpointType::Full, None).await.unwrap();
        api.create("other", CheckpointType::Full, None).await.unwrap();

        let removed = api.delete_for("loop-d").await.unwrap();
        assert_eq!(removed, 2);
        assert!(api.list("loop-d").await.unwrap().is_empty());
        assert_eq!(api.list("other").await.unwrap().len(), 1);
    }
}

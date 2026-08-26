use crate::delta::CheckpointLoader;
use crate::error::CheckpointError;
use crate::state::storage::StorageBackedStateManager;
use crate::state::CheckpointStateManager;
use std::sync::Arc;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::agent::AgentCheckpointDelta;
use wf_types::checkpoint::agent::AgentStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::storage::CheckpointStorageMetadata;

pub type AgentCheckpoint = BaseCheckpointCore<AgentCheckpointDelta, AgentStateSnapshot>;

pub struct AgentCheckpointStateManager {
    inner: StorageBackedStateManager<AgentCheckpoint>,
}

impl AgentCheckpointStateManager {
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self {
            inner: StorageBackedStateManager::new(storage),
        }
    }

    /// The underlying storage backend (used to rebuild state managers in
    /// spawned restore tasks).
    pub fn storage(&self) -> &Arc<StorageBackend> {
        self.inner.storage()
    }
}

impl CheckpointStateManager for AgentCheckpointStateManager {
    type Checkpoint = AgentCheckpoint;

    async fn save(
        &self,
        checkpoint: &Self::Checkpoint,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<(), CheckpointError> {
        self.inner.save(checkpoint, entity_type, entity_id).await
    }

    async fn load(&self, id: &str) -> Result<Option<Self::Checkpoint>, CheckpointError> {
        self.inner.load(id).await
    }

    async fn load_batch(&self, ids: &[String]) -> Result<Vec<Self::Checkpoint>, CheckpointError> {
        self.inner.load_batch(ids).await
    }

    async fn delete(&self, id: &str) -> Result<bool, CheckpointError> {
        self.inner.delete(id).await
    }

    async fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        self.inner.list_by_entity(entity_id).await
    }

    async fn get_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        self.inner.get_latest(entity_id).await
    }

    async fn count_by_entity(&self, entity_id: &str) -> Result<u64, CheckpointError> {
        self.inner.count_by_entity(entity_id).await
    }

    async fn list_by_entity_paged(
        &self,
        entity_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        self.inner
            .list_by_entity_paged(entity_id, offset, limit)
            .await
    }

    async fn cleanup(
        &self,
        entity_id: &str,
        max_count: Option<u32>,
    ) -> Result<u64, CheckpointError> {
        self.inner.cleanup(entity_id, max_count).await
    }
}

#[async_trait::async_trait]
impl CheckpointLoader for AgentCheckpointStateManager {
    async fn load_checkpoint_data(&self, id: &str) -> Result<Option<Vec<u8>>, CheckpointError> {
        self.inner.load_checkpoint_data(id).await
    }

    async fn load_metadata(
        &self,
        id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        self.inner.load_metadata(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_snapshot() -> AgentStateSnapshot {
        AgentStateSnapshot {
            agent_loop_id: "loop-1".to_string(),
            status: "running".to_string(),
            current_iteration: 1,
            tool_call_count: 0,
            conversation_snapshot: None,
            tool_call_history: None,
            is_streaming: None,
            variable_snapshots: None,
            error: None,
            started_at: None,
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

    fn make_checkpoint() -> AgentCheckpoint {
        BaseCheckpointCore {
            id: "cp-1".to_string(),
            r#type: None,
            base_checkpoint_id: None,
            previous_checkpoint_id: None,
            delta: None,
            snapshot: Some(make_snapshot()),
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
            metadata: None,
            format_version: None,
        }
    }

    #[tokio::test]
    async fn save_and_load_agent_checkpoint() {
        let storage = Arc::new(StorageBackend::new_memory());
        let mgr = AgentCheckpointStateManager::new(storage);

        let cp = make_checkpoint();
        mgr.save(&cp, "agent_loop", "loop-1").await.unwrap();

        let loaded = mgr.load("cp-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, "cp-1");
    }

    #[tokio::test]
    async fn agent_list_and_cleanup() {
        let storage = Arc::new(StorageBackend::new_memory());
        let mgr = AgentCheckpointStateManager::new(storage);

        for i in 0..4 {
            let mut cp = make_checkpoint();
            cp.id = format!("cp-{}", i);
            cp.timestamp = Some(i as i64 * 1000);
            mgr.save(&cp, "agent_loop", "loop-1").await.unwrap();
        }

        let all = mgr.list_by_entity("loop-1").await.unwrap();
        assert_eq!(all.len(), 4);

        let deleted = mgr.cleanup("loop-1", Some(2)).await.unwrap();
        assert_eq!(deleted, 2);
    }
}

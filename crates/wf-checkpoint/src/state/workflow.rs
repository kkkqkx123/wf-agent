use crate::delta::CheckpointLoader;
use crate::error::CheckpointError;
use crate::state::storage::StorageBackedStateManager;
use crate::state::CheckpointStateManager;
use std::sync::Arc;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::workflow::WorkflowCheckpointDelta;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::storage::CheckpointStorageMetadata;

pub type WorkflowCheckpoint =
    BaseCheckpointCore<WorkflowCheckpointDelta, WorkflowExecutionStateSnapshot>;

pub struct WorkflowCheckpointStateManager {
    inner: StorageBackedStateManager<WorkflowCheckpoint>,
}

impl WorkflowCheckpointStateManager {
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

    /// Resolve the latest checkpoint metadata for many entities in a single
    /// storage query (eliminates the N+1 pattern in hierarchy restore).
    pub async fn list_latest_by_entities(
        &self,
        entity_ids: &[String],
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        self.inner.list_latest_by_entities(entity_ids).await
    }
}

impl CheckpointStateManager for WorkflowCheckpointStateManager {
    type Checkpoint = WorkflowCheckpoint;

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

    async fn cleanup(
        &self,
        entity_id: &str,
        max_count: Option<u32>,
    ) -> Result<u64, CheckpointError> {
        self.inner.cleanup(entity_id, max_count).await
    }
}

#[async_trait::async_trait]
impl CheckpointLoader for WorkflowCheckpointStateManager {
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

    fn make_snapshot() -> WorkflowExecutionStateSnapshot {
        WorkflowExecutionStateSnapshot {
            execution_id: "exec-1".to_string(),
            status: "running".to_string(),
            current_node_id: Some("node-1".to_string()),
            node_results: None,
            variable_state: wf_types::checkpoint::CheckpointVariableState {
                variables: std::collections::HashMap::new(),
            },
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
        }
    }

    fn make_checkpoint() -> WorkflowCheckpoint {
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
    async fn save_and_load_workflow_checkpoint() {
        let storage = Arc::new(StorageBackend::new_memory());
        let mgr = WorkflowCheckpointStateManager::new(storage);

        let cp = make_checkpoint();
        mgr.save(&cp, "workflow_execution", "exec-1").await.unwrap();

        let loaded = mgr.load("cp-1").await.unwrap();
        assert!(loaded.is_some());
        let loaded_cp = loaded.unwrap();
        assert_eq!(loaded_cp.id, "cp-1");
        assert!(loaded_cp.snapshot.is_some());
    }

    #[tokio::test]
    async fn list_and_cleanup() {
        let storage = Arc::new(StorageBackend::new_memory());
        let mgr = WorkflowCheckpointStateManager::new(storage);

        for i in 0..3 {
            let mut cp = make_checkpoint();
            cp.id = format!("cp-{}", i);
            cp.timestamp = Some(i as i64 * 1000);
            mgr.save(&cp, "workflow_execution", "exec-1").await.unwrap();
        }

        let all = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(all.len(), 3);

        let deleted = mgr.cleanup("exec-1", Some(2)).await.unwrap();
        assert_eq!(deleted, 1);

        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(remaining.len(), 2);
    }
}

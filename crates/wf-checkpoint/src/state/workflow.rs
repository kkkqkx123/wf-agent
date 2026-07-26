use crate::error::CheckpointError;
use crate::state::CheckpointStateManager;
use async_trait::async_trait;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::workflow::WorkflowCheckpointDelta;
use wf_types::storage::CheckpointStorageMetadata;

pub type WorkflowCheckpoint = BaseCheckpointCore<WorkflowCheckpointDelta, WorkflowExecutionStateSnapshot>;

pub struct WorkflowCheckpointStateManager;

impl WorkflowCheckpointStateManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WorkflowCheckpointStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CheckpointStateManager for WorkflowCheckpointStateManager {
    type Checkpoint = WorkflowCheckpoint;

    async fn save(&self, _checkpoint: &Self::Checkpoint) -> Result<(), CheckpointError> {
        Ok(())
    }

    async fn load(&self, _id: &str) -> Result<Option<Self::Checkpoint>, CheckpointError> {
        Ok(None)
    }

    async fn delete(&self, _id: &str) -> Result<bool, CheckpointError> {
        Ok(false)
    }

    async fn list_by_entity(
        &self,
        _entity_id: &str,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        Ok(Vec::new())
    }

    async fn get_latest(
        &self,
        _entity_id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        Ok(None)
    }

    async fn cleanup(&self, _entity_id: &str, _max_count: Option<u32>) -> Result<u64, CheckpointError> {
        Ok(0)
    }
}

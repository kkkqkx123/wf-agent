use crate::error::CheckpointError;
use crate::state::CheckpointStateManager;
use async_trait::async_trait;
use wf_types::checkpoint::agent::AgentStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::agent::AgentCheckpointDelta;
use wf_types::storage::CheckpointStorageMetadata;

pub type AgentCheckpoint = BaseCheckpointCore<AgentCheckpointDelta, AgentStateSnapshot>;

pub struct AgentCheckpointStateManager;

impl AgentCheckpointStateManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentCheckpointStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CheckpointStateManager for AgentCheckpointStateManager {
    type Checkpoint = AgentCheckpoint;

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

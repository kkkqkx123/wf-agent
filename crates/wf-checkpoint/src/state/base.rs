use crate::error::CheckpointError;
use async_trait::async_trait;
use wf_types::storage::CheckpointStorageMetadata;

#[async_trait]
pub trait CheckpointStateManager: Send + Sync {
    type Checkpoint: Send + Sync;

    async fn save(
        &self,
        checkpoint: &Self::Checkpoint,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<(), CheckpointError>;
    async fn load(&self, id: &str) -> Result<Option<Self::Checkpoint>, CheckpointError>;
    async fn delete(&self, id: &str) -> Result<bool, CheckpointError>;
    async fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError>;
    async fn get_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError>;
    async fn cleanup(
        &self,
        entity_id: &str,
        max_count: Option<u32>,
    ) -> Result<u64, CheckpointError>;
}

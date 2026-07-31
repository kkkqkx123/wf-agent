use crate::error::CheckpointError;
use wf_types::storage::CheckpointStorageMetadata;

pub trait CheckpointStateManager: Send + Sync {
    type Checkpoint: Send + Sync;

    fn save(
        &self,
        checkpoint: &Self::Checkpoint,
        entity_type: &str,
        entity_id: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;
    fn load(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<Option<Self::Checkpoint>, CheckpointError>> + Send;
    fn delete(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<bool, CheckpointError>> + Send;
    fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<CheckpointStorageMetadata>, CheckpointError>> + Send;
    fn get_latest(
        &self,
        entity_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<CheckpointStorageMetadata>, CheckpointError>>
           + Send;
    fn cleanup(
        &self,
        entity_id: &str,
        max_count: Option<u32>,
    ) -> impl std::future::Future<Output = Result<u64, CheckpointError>> + Send;
}

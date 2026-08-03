use crate::cleanup::CleanupStrategy;
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
    fn load_batch(
        &self,
        ids: &[String],
    ) -> impl std::future::Future<Output = Result<Vec<Self::Checkpoint>, CheckpointError>> + Send;
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

    /// Clean up checkpoints of an entity with an explicit `CleanupStrategy`.
    /// The default implementation maps count-based strategies onto `cleanup`
    /// and keeps everything for other strategies; storage-backed managers
    /// override this with the full `CleanupExecutor` routing.
    fn cleanup_with_strategy(
        &self,
        entity_id: &str,
        strategy: &CleanupStrategy,
    ) -> impl std::future::Future<Output = Result<u64, CheckpointError>> + Send {
        async move {
            match strategy {
                CleanupStrategy::CountBased {
                    max_checkpoints,
                    min_retention: _,
                } => self.cleanup(entity_id, Some(*max_checkpoints as u32)).await,
                _ => self.cleanup(entity_id, None).await,
            }
        }
    }
}

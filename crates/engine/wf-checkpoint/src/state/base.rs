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

    /// Number of checkpoints persisted for an entity. Storage-backed
    /// managers implement this with an aggregate query; the default counts
    /// the full listing (metadata only, no payload reads).
    fn count_by_entity(
        &self,
        entity_id: &str,
    ) -> impl std::future::Future<Output = Result<u64, CheckpointError>> + Send {
        async move { Ok(self.list_by_entity(entity_id).await?.len() as u64) }
    }

    /// Paged listing of an entity's checkpoints (newest first). The default
    /// implementation pages the full listing in memory; storage-backed
    /// managers push offset/limit down to the backend.
    fn list_by_entity_paged(
        &self,
        entity_id: &str,
        offset: u64,
        limit: u64,
    ) -> impl std::future::Future<Output = Result<Vec<CheckpointStorageMetadata>, CheckpointError>> + Send
    {
        async move {
            let mut all = self.list_by_entity(entity_id).await?;
            all.reverse();
            let start = (offset as usize).min(all.len());
            let end = (start + limit as usize).min(all.len());
            Ok(all[start..end].to_vec())
        }
    }
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

use crate::error::CheckpointError;

#[async_trait::async_trait]
pub trait DiffCalculator<SS, DS>: Send + Sync
where
    SS: Send + Sync,
    DS: Send + Sync,
{
    async fn calculate_diff(&self, previous: &SS, current: &SS) -> Result<DS, CheckpointError>;
    async fn apply_delta(&self, base: &SS, delta: &DS) -> Result<SS, CheckpointError>;
}

#[async_trait::async_trait]
pub trait DeltaRestorer<SS, DS>: Send + Sync
where
    SS: Send + Sync,
    DS: Send + Sync,
{
    async fn restore_full_state(
        &self,
        target_checkpoint_id: &str,
        loader: &dyn CheckpointLoader,
    ) -> Result<SS, CheckpointError>;
}

#[async_trait::async_trait]
pub trait CheckpointLoader: Send + Sync {
    async fn load_checkpoint_data(&self, id: &str) -> Result<Option<Vec<u8>>, CheckpointError>;
    async fn load_metadata(
        &self,
        id: &str,
    ) -> Result<Option<wf_types::storage::CheckpointStorageMetadata>, CheckpointError>;
}

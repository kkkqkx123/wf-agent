use std::future::Future;

use crate::error::CheckpointError;

pub trait DiffCalculator<SS, DS>: Send + Sync
where
    SS: Send + Sync,
    DS: Send + Sync,
{
    fn calculate_diff(
        &self,
        previous: &SS,
        current: &SS,
    ) -> impl Future<Output = Result<DS, CheckpointError>> + Send;
    fn apply_delta(
        &self,
        base: &SS,
        delta: &DS,
    ) -> impl Future<Output = Result<SS, CheckpointError>> + Send;
}

pub trait DeltaRestorer<SS, DS>: Send + Sync
where
    SS: Send + Sync,
    DS: Send + Sync,
{
    fn restore_full_state(
        &self,
        target_checkpoint_id: &str,
        loader: &dyn CheckpointLoader,
    ) -> impl Future<Output = Result<SS, CheckpointError>> + Send;
}

#[async_trait::async_trait]
pub trait CheckpointLoader: Send + Sync {
    async fn load_checkpoint_data(&self, id: &str) -> Result<Option<Vec<u8>>, CheckpointError>;
    async fn load_metadata(
        &self,
        id: &str,
    ) -> Result<Option<wf_types::storage::CheckpointStorageMetadata>, CheckpointError>;
}

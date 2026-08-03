use crate::error::CheckpointError;

#[async_trait::async_trait]
pub trait DiffCalculator<SS, DS>: Send + Sync
where
    SS: Send + Sync,
    DS: Send + Sync,
{
    async fn calculate_diff(&self, previous: &SS, current: &SS) -> Result<DS, CheckpointError>;
    async fn apply_delta(&self, base: &SS, delta: &DS) -> Result<SS, CheckpointError>;

    /// Merge two consecutive deltas into a single one equivalent to applying
    /// `first` then `second` on top of `base`.
    ///
    /// The default implementation is correct by construction: it applies both
    /// deltas in sequence and re-diffs against the base. Implementors may
    /// override with a cheaper field-level merge if their delta format allows
    /// it.
    async fn merge_deltas(
        &self,
        base: &SS,
        first: &DS,
        second: &DS,
    ) -> Result<DS, CheckpointError> {
        let intermediate = self.apply_delta(base, first).await?;
        let current = self.apply_delta(&intermediate, second).await?;
        self.calculate_diff(base, &current).await
    }
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

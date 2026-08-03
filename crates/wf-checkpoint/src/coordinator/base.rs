use crate::error::CheckpointError;
use crate::strategy::CheckpointStrategy;
use wf_types::checkpoint::{CheckpointContext, CheckpointTrigger, DeltaStorageConfig};

pub trait CheckpointCoordinator: Send + Sync {
    type Checkpoint: Send + Sync;
    type Entity: Send + Sync;
    type State: Send + Sync;

    fn prepare(
        &self,
        entity_id: &str,
        trigger: CheckpointTrigger,
    ) -> impl std::future::Future<Output = Result<CheckpointContext, CheckpointError>> + Send;

    fn build(
        &self,
        ctx: CheckpointContext,
        state: Self::State,
    ) -> impl std::future::Future<Output = Result<Self::Checkpoint, CheckpointError>> + Send;

    fn persist(
        &self,
        checkpoint: &Self::Checkpoint,
        entity_id: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;

    fn restore(
        &self,
        checkpoint_id: &str,
    ) -> impl std::future::Future<Output = Result<Self::Entity, CheckpointError>> + Send;

    fn delete(
        &self,
        checkpoint_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, CheckpointError>> + Send;

    fn determine_type(
        &self,
        entity_id: &str,
        config: &DeltaStorageConfig,
    ) -> impl std::future::Future<
        Output = Result<wf_types::checkpoint::CheckpointType, CheckpointError>,
    > + Send;

    /// The strategy used by `create_checkpoint_with_strategy` to decide
    /// whether a checkpoint should be created for a trigger. `None` means
    /// every request is accepted.
    fn default_strategy(&self) -> Option<&dyn CheckpointStrategy>;

    /// Create a checkpoint guarded by the default strategy: when the strategy
    /// rejects the trigger, no checkpoint is produced (returns `Ok(None)`).
    /// This aligns with the TS `createCheckpointWithStrategy` semantics.
    fn create_checkpoint_with_strategy(
        &self,
        trigger: CheckpointTrigger,
        entity_id: &str,
        state: Self::State,
    ) -> impl std::future::Future<Output = Result<Option<Self::Checkpoint>, CheckpointError>> + Send
    {
        async move {
            let ctx = self.prepare(entity_id, trigger.clone()).await?;
            if let Some(strategy) = self.default_strategy() {
                if !strategy.should_checkpoint(&trigger, &ctx) {
                    return Ok(None);
                }
            }
            let checkpoint = self.build(ctx, state).await?;
            Ok(Some(checkpoint))
        }
    }
}

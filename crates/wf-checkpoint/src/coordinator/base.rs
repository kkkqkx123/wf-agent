use crate::error::CheckpointError;
use crate::strategy::CheckpointStrategy;
use wf_types::checkpoint::{CheckpointContext, CheckpointTrigger, DeltaStorageConfig};

pub trait CheckpointCoordinator: Send + Sync {
    type Checkpoint: Send + Sync + serde::Serialize;
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

    /// Structural validation of a checkpoint before it is persisted or
    /// restored (aligned with TS `BaseCheckpointCoordinator.validateCheckpoint`):
    /// a FULL checkpoint must carry a snapshot, a DELTA checkpoint must carry
    /// `base_checkpoint_id` + `previous_checkpoint_id` + a delta.
    fn validate_checkpoint(
        &self,
        checkpoint: &Self::Checkpoint,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send;

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

    /// Best-effort file snapshot hook invoked by `create_checkpoint` after
    /// the checkpoint has been persisted. The default is a no-op; engine
    /// integrations (layertwine / file-history adapters) override it. Errors
    /// are logged by `create_checkpoint` and never fail the create flow.
    fn save_file_snapshot(
        &self,
        _checkpoint_id: &str,
        _entity_id: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send {
        async move { Ok(()) }
    }

    /// Whether post-persist side effects are deferred to a background
    /// persistence queue (TS `contentConfig.async`). When `true`,
    /// `create_checkpoint` enqueues the side effects via
    /// `enqueue_persistence` instead of running them inline, so the
    /// checkpoint id is returned before they complete. Default: `false`.
    fn async_persistence_enabled(&self) -> bool {
        false
    }

    /// Wait for all deferred persistence operations to complete. Call this
    /// before critical operations that require checkpoint durability
    /// (TS `waitForPersistence`). The default is a no-op.
    fn wait_for_persistence(&self) -> impl std::future::Future<Output = ()> + Send {
        async move {}
    }

    /// Defer post-persist side effects to the background persistence queue.
    /// The default runs them inline (equivalent to synchronous mode).
    fn enqueue_persistence(
        &self,
        checkpoint_id: &str,
        entity_id: &str,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
            if let Err(err) = self.save_file_snapshot(checkpoint_id, entity_id).await {
                tracing::warn!(
                    entity_id = %entity_id,
                    checkpoint_id = %checkpoint_id,
                    error = %err,
                    "deferred file checkpoint creation failed (best-effort)"
                );
            }
        }
    }

    /// Create, validate and persist a checkpoint for the trigger, returning
    /// the saved checkpoint id. This is the aggregate entry point aligned
    /// with TS `createCheckpoint`: prepare -> build -> validate -> persist ->
    /// best-effort file snapshot.
    fn create_checkpoint(
        &self,
        trigger: CheckpointTrigger,
        entity_id: &str,
        state: Self::State,
    ) -> impl std::future::Future<Output = Result<String, CheckpointError>> + Send {
        async move {
            let ctx = self.prepare(entity_id, trigger).await?;
            let checkpoint = self.build(ctx, state).await?;
            self.validate_checkpoint(&checkpoint).await?;
            let checkpoint_id = checkpoint_id_of(&checkpoint);
            self.persist(&checkpoint, entity_id).await?;
            if self.async_persistence_enabled() {
                self.enqueue_persistence(&checkpoint_id, entity_id).await;
            } else if let Err(err) = self.save_file_snapshot(&checkpoint_id, entity_id).await {
                tracing::warn!(
                    entity_id = %entity_id,
                    checkpoint_id = %checkpoint_id,
                    error = %err,
                    "file checkpoint creation failed (best-effort)"
                );
            }
            Ok(checkpoint_id)
        }
    }

    /// Create a checkpoint guarded by the default strategy: when the strategy
    /// rejects the trigger, no checkpoint is produced (returns `Ok(None)`).
    /// The checkpoint is persisted when created, and the saved id is
    /// returned. This aligns with the TS `createCheckpointWithStrategy`
    /// semantics.
    fn create_checkpoint_with_strategy(
        &self,
        trigger: CheckpointTrigger,
        entity_id: &str,
        state: Self::State,
    ) -> impl std::future::Future<Output = Result<Option<String>, CheckpointError>> + Send {
        async move {
            let ctx = self.prepare(entity_id, trigger.clone()).await?;
            if let Some(strategy) = self.default_strategy() {
                if !strategy.should_checkpoint(&trigger, &ctx) {
                    return Ok(None);
                }
            }
            let id = self.create_checkpoint(trigger, entity_id, state).await?;
            Ok(Some(id))
        }
    }
}

/// Extract the checkpoint id for event/metadata correlation. Serialization
/// fallback keeps generic (JSON-serializable) checkpoints working; the
/// default is an empty id.
fn checkpoint_id_of<C: serde::Serialize>(checkpoint: &C) -> String {
    serde_json::to_value(checkpoint)
        .ok()
        .and_then(|json| {
            json.get("id")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_default()
}

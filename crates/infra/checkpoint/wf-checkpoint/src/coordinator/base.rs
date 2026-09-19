use checkpoint_base::error::CheckpointError;
use checkpoint_base::strategy::CheckpointStrategy;
use checkpoint_file::event::CheckpointEventBus;
use wf_types::checkpoint::{
    CheckpointContext, CheckpointTiming, CheckpointType, DeltaStorageConfig,
};
use wf_types::execution::ExecutionStatus;

/// Shared storage-type decision: aggregate COUNT query semantics live in the
/// caller, this helper only maps count to Full/Delta.
pub fn decide_checkpoint_type_by_count(count: u64, config: &DeltaStorageConfig) -> CheckpointType {
    if !config.enabled {
        return CheckpointType::Full;
    }
    let effective_interval = config.baseline_interval.min(config.max_delta_chain_length);
    if count == 0 || effective_interval == 0 || count.is_multiple_of(effective_interval as u64) {
        CheckpointType::Full
    } else {
        CheckpointType::Delta
    }
}

/// Shared chain-position rule: Full resets to zero, Delta increments.
pub fn next_chain_position(
    checkpoint_type: &CheckpointType,
    previous_position: Option<u32>,
) -> u32 {
    match checkpoint_type {
        CheckpointType::Full => 0,
        CheckpointType::Delta => previous_position.map(|p| p + 1).unwrap_or(1),
    }
}

/// Shared persist event publishing so agent and workflow report identically.
pub fn publish_persisted(
    bus: Option<&CheckpointEventBus>,
    checkpoint_id: &str,
    entity_id: &str,
    description: Option<&str>,
) {
    if let Some(bus) = bus {
        bus.publish(CheckpointEventBus::created_with(
            checkpoint_id.to_string(),
            Some(entity_id.to_string()),
            description.map(String::from),
        ));
    }
}

/// Shared persist-failure event publishing.
pub fn publish_persist_failed(
    bus: Option<&CheckpointEventBus>,
    checkpoint_id: Option<String>,
    entity_id: &str,
    err: &CheckpointError,
) {
    if let Some(bus) = bus {
        bus.publish(CheckpointEventBus::failed_with(
            checkpoint_id,
            "create",
            format!("persist failed: {}", err),
            Some(entity_id.to_string()),
        ));
    }
}

pub trait CheckpointCoordinator: Send + Sync {
    type Checkpoint: Send + Sync + serde::Serialize;
    type Entity: Send + Sync;
    type State: Send + Sync;

    fn prepare(
        &self,
        entity_id: &str,
        trigger: CheckpointTiming,
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
    /// restored:
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
    /// are logged with the state checkpoint id for correlation and never
    /// fail the create flow.
    fn save_file_snapshot(
        &self,
        checkpoint_id: &str,
        _entity_id: &str,
    ) -> impl std::future::Future<Output = Result<(), CheckpointError>> + Send {
        async move {
            let _ = checkpoint_id;
            Ok(())
        }
    }

    /// Whether post-persist side effects are deferred to a background
    /// persistence queue (`contentConfig.async`). When `true`,
    /// `create_checkpoint` enqueues the side effects via
    /// `enqueue_persistence` instead of running them inline, so the
    /// checkpoint id is returned before they complete. Default: `false`.
    fn async_persistence_enabled(&self) -> bool {
        false
    }

    /// Wait for all deferred persistence operations to complete. Call this
    /// before critical operations that require checkpoint durability
    /// (`waitForPersistence`). The default is a no-op.
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
    /// the saved checkpoint id. This is the aggregate entry point:
    /// prepare -> build -> validate -> persist ->
    /// best-effort file snapshot.
    fn create_checkpoint(
        &self,
        trigger: CheckpointTiming,
        entity_id: &str,
        state: Self::State,
    ) -> impl std::future::Future<Output = Result<String, CheckpointError>> + Send {
        async move {
            let lifecycle = is_lifecycle_trigger(&trigger);
            let ctx = self.prepare(entity_id, trigger).await?;
            let checkpoint = self.build(ctx, state).await?;
            self.validate_checkpoint(&checkpoint).await?;
            let checkpoint_id = checkpoint_id_of(&checkpoint);
            self.persist(&checkpoint, entity_id).await?;
            if self.async_persistence_enabled() {
                self.enqueue_persistence(&checkpoint_id, entity_id).await;
            } else if let Err(err) = self.save_file_snapshot(&checkpoint_id, entity_id).await {
                if lifecycle {
                    tracing::error!(
                        entity_id = %entity_id,
                        checkpoint_id = %checkpoint_id,
                        error = %err,
                        "file projection failed for lifecycle checkpoint (best-effort)"
                    );
                } else {
                    tracing::warn!(
                        entity_id = %entity_id,
                        checkpoint_id = %checkpoint_id,
                        error = %err,
                        "file checkpoint creation failed (best-effort)"
                    );
                }
            }
            Ok(checkpoint_id)
        }
    }

    /// Create a checkpoint guarded by the default strategy: when the strategy
    /// rejects the trigger, no checkpoint is produced (returns `Ok(None)`).
    /// The checkpoint is persisted when created, and the saved id is
    /// returned.
    fn create_checkpoint_with_strategy(
        &self,
        trigger: CheckpointTiming,
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

    /// Whether a manual checkpoint is allowed. Manual creation honors the
    /// master switch only and bypasses the per-trigger whitelist and
    /// cadence, so it stays available even when the policy omits the manual
    /// trigger. No strategy means every manual request is accepted.
    fn manual_allowed(&self) -> bool {
        self.default_strategy()
            .map(|strategy| strategy.manual_allowed())
            .unwrap_or(true)
    }

    /// Create a manual checkpoint: rejects only when the master switch is
    /// off, otherwise delegates to the unguarded aggregate entry point.
    fn create_manual_checkpoint(
        &self,
        entity_id: &str,
        state: Self::State,
    ) -> impl std::future::Future<Output = Result<String, CheckpointError>> + Send {
        async move {
            if !self.manual_allowed() {
                return Err(CheckpointError::Strategy(
                    "manual checkpoint rejected: checkpointing is disabled".to_string(),
                ));
            }
            self.create_checkpoint(CheckpointTiming::Manual, entity_id, state)
                .await
        }
    }
}

/// Whether the trigger marks an execution lifecycle boundary. Lifecycle
/// checkpoints honor the master switch but bypass per-trigger cadence, so
/// both coordinators classify the same trigger set identically.
pub fn is_lifecycle_trigger(trigger: &CheckpointTiming) -> bool {
    matches!(
        trigger,
        CheckpointTiming::Manual
            | CheckpointTiming::OnComplete
            | CheckpointTiming::OnPause
            | CheckpointTiming::OnCancel
            | CheckpointTiming::OnTimeout
            | CheckpointTiming::OnStopped
            | CheckpointTiming::OnFailure
    )
}

/// Upper bound on deferred persistence queues shared by both coordinators.
pub const MAX_PERSISTENCE_QUEUE: usize = 128;

/// Push a background persistence handle, awaiting the backlog first when
/// the queue is full so memory stays bounded. Shared by both coordinators.
pub async fn push_persistence_handle(
    queue: &std::sync::Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    handle: tokio::task::JoinHandle<()>,
) {
    let mut guard = queue.lock().await;
    if guard.len() >= MAX_PERSISTENCE_QUEUE {
        let backlog: Vec<_> = std::mem::take(&mut *guard);
        drop(guard);
        for task in backlog {
            if let Err(join_err) = task.await {
                tracing::warn!(error = %join_err, "persistence task panicked");
            }
        }
        guard = queue.lock().await;
    }
    guard.push(handle);
}

/// Drain all deferred persistence handles. Shared by both coordinators.
pub async fn drain_persistence_handles(
    queue: &std::sync::Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
) {
    let handles: Vec<_> = {
        let mut guard = queue.lock().await;
        std::mem::take(&mut *guard)
    };
    for handle in handles {
        if let Err(join_err) = handle.await {
            tracing::warn!(error = %join_err, "persistence task panicked");
        }
    }
}

/// Read the execution status carried by a restored checkpoint payload.
///
/// Shared by the agent-loop and workflow coordinators so both resolve the
/// status field identically. Returns `None` when the payload carries no
/// usable status string, letting the caller apply its own default.
pub fn restored_status(value: &serde_json::Value) -> Option<ExecutionStatus> {
    value
        .get("status")
        .and_then(|status| status.as_str())
        .map(ExecutionStatus::from_wire)
}

/// Extract the checkpoint id for event/metadata correlation. Serialization
/// fallback keeps generic (JSON-serializable) checkpoints working; the
/// default is an empty id.
fn checkpoint_id_of<C: serde::Serialize>(checkpoint: &C) -> String {
    serde_json::to_value(checkpoint)
        .ok()
        .and_then(|json| json.get("id").and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_default()
}

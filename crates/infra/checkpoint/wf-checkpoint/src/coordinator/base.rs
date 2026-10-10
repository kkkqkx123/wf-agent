use crate::coordinator::events::publish_cleanup_skipped;
use checkpoint_base::error::CheckpointError;
use checkpoint_base::strategy::CheckpointStrategy;
use checkpoint_file::event::CheckpointEventBus;
use checkpoint_file::file::FileCheckpointManager;
use checkpoint_state::state::CheckpointStateManager;
use std::collections::HashMap;
use wf_types::checkpoint::{
    BaseCheckpointCore, CheckpointContext, CheckpointTiming, CheckpointType, DeltaStorageConfig,
};
use wf_types::storage::CheckpointStorageMetadata;

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

/// Build the checkpoint creation context shared by both coordinators. The
/// file checkpoint side effect has observable effects: the actor id is
/// resolved hierarchically, so calling this twice for one creation wastes work.
pub async fn prepare_context(
    file_checkpoint_manager: Option<&FileCheckpointManager>,
    entity_type: &str,
    entity_id: &str,
    trigger: CheckpointTiming,
    parent_execution_id: Option<&str>,
    ancestors: Option<&[String]>,
) -> Result<CheckpointContext, CheckpointError> {
    let actor_id = file_checkpoint_manager.map(|manager| match ancestors {
        Some(ancestors) => manager
            .resolve_actor_with_chain(entity_id, ancestors, parent_execution_id)
            .to_string(),
        None => manager
            .resolve_actor(entity_id, parent_execution_id)
            .to_string(),
    });
    Ok(CheckpointContext {
        entity_type: entity_type.to_string(),
        entity_id: entity_id.to_string(),
        trigger: Some(trigger),
        actor_id,
        attempt: None,
        retry_count: None,
        error: None,
        fallback_used: None,
        metadata: None,
    })
}

/// Assemble the wire custom fields: caller fields first, then the generated
/// chain-position and progress-coordinate fields, which win over a same-named
/// caller field so coordinates always describe the real execution state.
/// Both coordinators stamp the same wire shape.
pub fn stamp_custom_fields(
    caller: Option<HashMap<String, serde_json::Value>>,
    generated: HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    let mut fields = caller.unwrap_or_default();
    for (key, value) in generated {
        fields.insert(key, value);
    }
    fields
}

/// Read-modify-write description merge shared by the agent-loop and
/// workflow coordinators: rewrites the caller-supplied text under
/// `customFields.description` on the stored blob without allocating a new
/// row. The trigger label (`metadata.description`) and every other field are
/// untouched, and the blob timestamp is preserved so chain order never
/// shifts. When cleanup removed the target between the gate read and this
/// write, an explicit not-found error is reported so callers never mistake a
/// lost race for a successful merge.
pub async fn merge_description_back<M>(
    manager: &M,
    checkpoint_id: &str,
    entity_type: &str,
    entity_id: &str,
    description: &str,
    bus: Option<&CheckpointEventBus>,
) -> Result<CheckpointStorageMetadata, CheckpointError>
where
    M: CheckpointStateManager,
    M::Checkpoint: CheckpointBlob,
{
    let Some(mut checkpoint) = manager.load(checkpoint_id).await? else {
        publish_cleanup_skipped(
            bus,
            Some(checkpoint_id.to_string()),
            entity_id,
            "cleanup_skip",
            "merge target already cleaned up",
        );
        return Err(CheckpointError::NotFound {
            id: checkpoint_id.to_string(),
        });
    };
    let metadata = checkpoint.blob_metadata_mut().get_or_insert_default();
    match metadata
        .get_mut("customFields")
        .and_then(|v| v.as_object_mut())
    {
        Some(custom) => {
            custom.insert("description".to_string(), serde_json::json!(description));
        }
        None => {
            metadata.insert(
                "customFields".to_string(),
                serde_json::json!({ "description": description }),
            );
        }
    }
    manager.save(&checkpoint, entity_type, entity_id).await?;
    if let Some(meta) = manager.load_metadata(checkpoint_id).await? {
        return Ok(meta);
    }
    publish_cleanup_skipped(
        bus,
        Some(checkpoint_id.to_string()),
        entity_id,
        "cleanup_skip",
        "merged row cleaned up before re-read",
    );
    Err(CheckpointError::NotFound {
        id: checkpoint_id.to_string(),
    })
}

/// Shared duplicate-gate reuse: when progress coordinates compare equal the
/// caller reuses the latest row instead of persisting a duplicate. A changed
/// description merges back into the latest row; an unchanged or absent
/// description keeps the latest id. Merge failures are reported loudly so a
/// metadata write never silently blocks checkpointing.
pub async fn reuse_duplicate_checkpoint<M>(
    manager: &M,
    latest: &CheckpointStorageMetadata,
    entity_type: &str,
    entity_id: &str,
    description: Option<&str>,
    bus: Option<&CheckpointEventBus>,
) -> Result<String, CheckpointError>
where
    M: CheckpointStateManager,
    M::Checkpoint: CheckpointBlob,
{
    if let Some(text) = description {
        let current = latest
            .custom_fields
            .as_ref()
            .and_then(|fields| fields.get("description"))
            .and_then(|v| v.as_str());
        if current != Some(text) {
            let merged =
                merge_description_back(manager, &latest.id, entity_type, entity_id, text, bus)
                    .await?;
            return Ok(merged.id);
        }
    }
    Ok(latest.id.clone())
}

/// Blob surface both coordinators and the shared helpers need: the mutable
/// metadata map, the caller description stored under it, and the row id.
/// Both checkpoint types share the same core shape, so one blanket
/// implementation covers them.
pub trait CheckpointBlob: Send + Sync {
    /// Trigger description carried by the stored wire metadata, published
    /// with the persist event.
    fn blob_description(&self) -> Option<&str>;

    /// Mutable wire metadata map.
    fn blob_metadata_mut(&mut self) -> &mut Option<HashMap<String, serde_json::Value>>;
}

/// Direct checkpoint identity without serialization round-trips.
/// Coordinators implement this with field access; the serialization fallback
/// below is only for generic contexts.
pub trait CheckpointId: Send + Sync {
    fn checkpoint_id(&self) -> &str;
}

impl<TDelta, TSnapshot> CheckpointBlob for BaseCheckpointCore<TDelta, TSnapshot>
where
    TDelta: Send + Sync,
    TSnapshot: Send + Sync,
{
    fn blob_description(&self) -> Option<&str> {
        self.metadata
            .as_ref()
            .and_then(|m| m.get("description"))
            .and_then(|v| v.as_str())
    }

    fn blob_metadata_mut(&mut self) -> &mut Option<HashMap<String, serde_json::Value>> {
        &mut self.metadata
    }
}

impl<TDelta, TSnapshot> CheckpointId for BaseCheckpointCore<TDelta, TSnapshot>
where
    TDelta: Send + Sync,
    TSnapshot: Send + Sync,
{
    fn checkpoint_id(&self) -> &str {
        &self.id
    }
}

pub trait CheckpointCoordinator: Send + Sync {
    type Checkpoint: Send + Sync + serde::Serialize + CheckpointId;
    type Entity: Send + Sync;
    type State: Send + Sync;

    /// Build the checkpoint context for an entity. This has side effects:
    /// it resolves the actor id, so calling it twice for one creation
    /// wastes work and must be avoided by reusing a single prepared context
    /// per creation.
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

    /// Attached event bus for best-effort failure reporting. Best-effort
    /// branches (async projection, persistence queue, cleanup races) publish
    /// Failed events through it while staying non-fatal.
    fn event_bus(&self) -> Option<&CheckpointEventBus> {
        None
    }

    /// Best-effort file snapshot hook invoked by `create_checkpoint` after
    /// the checkpoint has been persisted. The default is a no-op; engine
    /// integrations (sqlite / file-history adapters) override it. Errors
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
                crate::coordinator::events::publish_best_effort_failed(
                    self.event_bus(),
                    Some(checkpoint_id.to_string()),
                    entity_id,
                    "async_projection",
                    &format!("deferred file checkpoint creation failed: {err}"),
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
            let ctx = self.prepare(entity_id, trigger.clone()).await?;
            self.create_checkpoint_with_context(trigger, ctx, entity_id, state)
                .await
        }
    }

    /// Shared creation body over an already prepared context, so strategy
    /// gating and creation never prepare twice for one checkpoint.
    fn create_checkpoint_with_context(
        &self,
        trigger: CheckpointTiming,
        ctx: CheckpointContext,
        entity_id: &str,
        state: Self::State,
    ) -> impl std::future::Future<Output = Result<String, CheckpointError>> + Send {
        async move {
            let lifecycle = is_lifecycle_trigger(&trigger);
            let checkpoint = self.build(ctx, state).await?;
            self.validate_checkpoint(&checkpoint).await?;
            let checkpoint_id = checkpoint.checkpoint_id().to_string();
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
                crate::coordinator::events::publish_best_effort_failed(
                    self.event_bus(),
                    Some(checkpoint_id.clone()),
                    entity_id,
                    "async_projection",
                    &format!("file projection failed: {err}"),
                );
            }
            Ok(checkpoint_id)
        }
    }

    /// Create a checkpoint guarded by the default strategy: when the strategy
    /// rejects the trigger, no checkpoint is produced (returns `Ok(None)`).
    /// The checkpoint is persisted when created, and the saved id is
    /// returned. Preparation runs exactly once: the same context gates the
    /// strategy and builds the checkpoint.
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
            self.create_checkpoint_with_context(trigger, ctx, entity_id, state)
                .await
                .map(Some)
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

/// Shared synchronous metadata index over pre-built checkpoint metadata
/// for child discovery breadth-first traversal, which runs synchronously.
pub struct ChildDiscoveryIndex {
    index: HashMap<String, CheckpointStorageMetadata>,
}

impl ChildDiscoveryIndex {
    pub fn new(index: HashMap<String, CheckpointStorageMetadata>) -> Self {
        Self { index }
    }
}

impl checkpoint_state::restore::ChildDiscoveryLoader for ChildDiscoveryIndex {
    fn load_child_metadata(
        &self,
        id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        Ok(self.index.get(id).cloned())
    }
}

pub type ChildMetadataIndex = ChildDiscoveryIndex;

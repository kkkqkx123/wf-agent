mod content_policy;
mod progress;
mod restore;
#[cfg(test)]
mod tests;

pub use progress::{snapshot_workflow_coords, workflow_progress_coords, WorkflowProgressCoords};

use crate::coordinator::base::{
    decide_checkpoint_type_by_count, next_chain_position, prepare_context, stamp_custom_fields,
    CheckpointCoordinator,
};
use crate::coordinator::persist::{delete_checkpoint, persist_checkpoint};
use crate::coordinator::projection::{
    enqueue_persistence, restore_state_files, save_file_snapshot,
};
use crate::coordinator::queue::{drain_persistence_handles, PersistenceQueue};
use crate::coordinator::workflow::progress::progress_custom_fields;
use checkpoint_base::clock::CheckpointClock;
use checkpoint_base::delta::DeltaRestorer;
use checkpoint_base::delta::DiffCalculator;
use checkpoint_base::delta::GenericDeltaRestorer;
use checkpoint_base::delta::WorkflowDiffCalculator;
use checkpoint_base::error::CheckpointError;
use checkpoint_base::metadata::builder::{
    build_checkpoint_metadata, trigger_description, trigger_tag,
};
use checkpoint_base::strategy::CheckpointStrategy;
use checkpoint_base::strategy::StandardStrategy;
use checkpoint_base::version_manager::VersionManager;
use checkpoint_file::event::CheckpointEventBus;
use checkpoint_file::file::FileCheckpointManager;
use checkpoint_state::restore::hierarchy::ChildDiscoverySummary;
use checkpoint_state::restore::registry::RestoreStrategyRegistry;
use checkpoint_state::state::CheckpointStateManager;
use checkpoint_state::state::WorkflowCheckpoint;
use checkpoint_state::state::WorkflowCheckpointStateManager;
use std::collections::HashSet;
use std::sync::Arc;
use wf_types::checkpoint::workflow::WorkflowCheckpointDelta;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::CheckpointContext;
use wf_types::checkpoint::CheckpointTiming;
use wf_types::checkpoint::CheckpointType;
use wf_types::checkpoint::DeltaStorageConfig;
use wf_types::checkpoint::UnifiedCheckpointPolicy;
use wf_types::storage::CheckpointStorageMetadata;

pub struct WorkflowCheckpointCoordinator {
    state_manager: WorkflowCheckpointStateManager,
    diff_calculator:
        Arc<dyn DiffCalculator<WorkflowExecutionStateSnapshot, WorkflowCheckpointDelta>>,
    event_bus: Option<CheckpointEventBus>,
    delta_config: DeltaStorageConfig,
    version_manager: VersionManager,
    clock: CheckpointClock,
    strategy: Option<StandardStrategy>,
    error_handler: checkpoint_base::error_handling::CheckpointErrorHandler,
    restore_registry: Option<RestoreStrategyRegistry>,
    file_checkpoint_manager: Option<FileCheckpointManager>,
    /// `contentConfig.async`: defer post-persist side effects to the
    /// background persistence queue.
    async_persistence: bool,
    /// Background persistence queue (`persistenceQueue`); drained by
    /// `wait_for_persistence`.
    persistence_queue: PersistenceQueue,
}

impl WorkflowCheckpointCoordinator {
    pub fn new(state_manager: WorkflowCheckpointStateManager) -> Self {
        Self {
            state_manager,
            diff_calculator: Arc::new(WorkflowDiffCalculator::new()),
            event_bus: None,
            delta_config: DeltaStorageConfig::default(),
            version_manager: VersionManager::new(),
            clock: CheckpointClock::system(),
            strategy: None,
            error_handler: checkpoint_base::error_handling::CheckpointErrorHandler::default(),
            restore_registry: None,
            file_checkpoint_manager: None,
            async_persistence: false,
            persistence_queue: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn with_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_delta_config(mut self, config: DeltaStorageConfig) -> Self {
        self.delta_config = config;
        self
    }

    pub fn with_version_manager(mut self, manager: VersionManager) -> Self {
        self.version_manager = manager;
        self
    }

    pub fn with_clock(mut self, clock: CheckpointClock) -> Self {
        self.clock = clock;
        self
    }

    pub fn clock(&self) -> &CheckpointClock {
        &self.clock
    }

    /// Configure the default checkpoint strategy from a unified policy.
    /// A disabled policy yields a strategy that never checkpoints. The
    /// policy's `content.async` flag also enables async persistence mode.
    /// The policy's retention compression is applied to the state manager so
    /// the save path honors it instead of silently staying on `Auto`.
    pub fn with_strategy(mut self, policy: &UnifiedCheckpointPolicy) -> Self {
        self.strategy = Some(checkpoint_base::strategy::create_checkpoint_strategy(
            policy,
        ));
        if let Some(ref strategy) = self.strategy {
            self.state_manager
                .set_compression(strategy.compression_strategy());
        }
        self.async_persistence = policy
            .content
            .as_ref()
            .and_then(|c| c.asynchronous)
            .unwrap_or(false);
        self
    }

    /// Enable async (non-blocking) checkpoint creation (`contentConfig.async`):
    /// post-persist side effects run on the background persistence queue and
    /// the checkpoint id is returned immediately. Use `wait_for_persistence`
    /// to ensure the queue is drained before durability-critical operations.
    pub fn with_async_persistence(mut self, enabled: bool) -> Self {
        self.async_persistence = enabled;
        self
    }

    /// Number of pending persistence tasks in the async queue.
    pub async fn pending_persistence_count(&self) -> usize {
        self.persistence_queue.lock().await.len()
    }

    /// Configure the checkpoint error handler (default: `warn`, non-fatal).
    pub fn with_error_handler(
        mut self,
        handler: checkpoint_base::error_handling::CheckpointErrorHandler,
    ) -> Self {
        self.error_handler = handler;
        self
    }

    /// Configure the error handler from a unified policy.
    pub fn with_error_policy(mut self, policy: &UnifiedCheckpointPolicy) -> Self {
        self.error_handler =
            checkpoint_base::error_handling::CheckpointErrorHandler::from_policy(policy);
        self
    }

    /// Register restore strategies used for child execution recovery in the
    /// post-restore phase.
    pub fn with_restore_registry(mut self, registry: RestoreStrategyRegistry) -> Self {
        self.restore_registry = Some(registry);
        self
    }

    /// Register the file checkpoint manager used to restore the latest file
    /// checkpoint for the entity after restore.
    pub fn with_file_checkpoint_manager(mut self, manager: FileCheckpointManager) -> Self {
        self.file_checkpoint_manager = Some(manager);
        self
    }

    pub fn state_manager(&self) -> &WorkflowCheckpointStateManager {
        &self.state_manager
    }

    pub fn version_manager(&self) -> &VersionManager {
        &self.version_manager
    }

    /// [`CheckpointCoordinator::prepare`] with the immediate parent execution
    /// id (sub-execution isolation): the actor id is resolved hierarchically
    /// when the parent is known.
    pub async fn prepare_with_parent(
        &self,
        entity_id: &str,
        trigger: CheckpointTiming,
        parent_execution_id: Option<&str>,
    ) -> Result<CheckpointContext, CheckpointError> {
        prepare_context(
            self.file_checkpoint_manager.as_ref(),
            "workflow_execution",
            entity_id,
            trigger,
            parent_execution_id,
            None,
        )
        .await
    }

    /// [`CheckpointCoordinator::prepare`] with the full ancestor chain of the
    /// execution, so the actor id is resolved hierarchically.
    pub async fn prepare_with_hierarchy(
        &self,
        entity_id: &str,
        trigger: CheckpointTiming,
        parent_execution_id: Option<&str>,
        ancestors: &[String],
    ) -> Result<CheckpointContext, CheckpointError> {
        prepare_context(
            self.file_checkpoint_manager.as_ref(),
            "workflow_execution",
            entity_id,
            trigger,
            parent_execution_id,
            Some(ancestors),
        )
        .await
    }

    /// Merge a caller-supplied description into an existing checkpoint row
    /// without allocating a new row. Shared implementation lives in
    /// [`crate::coordinator::base::merge_description_back`]; the contract
    /// (trigger label untouched, timestamp preserved, missing target reports
    /// not-found) is identical for both coordinators.
    pub async fn merge_description_back(
        &self,
        checkpoint_id: &str,
        entity_id: &str,
        description: &str,
    ) -> Result<CheckpointStorageMetadata, CheckpointError> {
        let merged = crate::coordinator::base::merge_description_back(
            &self.state_manager,
            checkpoint_id,
            "workflow_execution",
            entity_id,
            description,
            self.event_bus.as_ref(),
        )
        .await?;
        Ok(merged)
    }

    pub async fn reuse_duplicate(
        &self,
        latest: &CheckpointStorageMetadata,
        entity_id: &str,
        description: Option<&str>,
    ) -> Result<String, CheckpointError> {
        crate::coordinator::base::reuse_duplicate_checkpoint(
            &self.state_manager,
            latest,
            "workflow_execution",
            entity_id,
            description,
            self.event_bus.as_ref(),
        )
        .await
    }

    /// Nearest previous checkpoint that still carries a full snapshot (the
    /// chain base); deltas in between have no snapshot of their own.
    async fn find_base(
        &self,
        previous: &Option<CheckpointStorageMetadata>,
    ) -> Result<(Option<String>, Option<WorkflowExecutionStateSnapshot>), CheckpointError> {
        let mut base_id: Option<String> = None;
        let mut base_snapshot: Option<WorkflowExecutionStateSnapshot> = None;
        let mut cursor: Option<String> = previous.as_ref().map(|p| p.id.clone());
        let mut visited: HashSet<String> = HashSet::new();

        while let Some(id) = cursor {
            if !visited.insert(id.clone()) {
                break;
            }
            match self.state_manager.load(&id).await? {
                Some(cp) if cp.snapshot.is_some() => {
                    base_id = Some(id);
                    base_snapshot = cp.snapshot;
                    break;
                }
                Some(cp) => cursor = cp.previous_checkpoint_id,
                None => break,
            }
        }

        Ok((base_id, base_snapshot))
    }
}

impl CheckpointCoordinator for WorkflowCheckpointCoordinator {
    type Checkpoint = WorkflowCheckpoint;
    type Entity = WorkflowExecutionEntity;
    type State = WorkflowExecutionStateSnapshot;

    fn async_persistence_enabled(&self) -> bool {
        self.async_persistence
    }

    async fn save_file_snapshot(
        &self,
        checkpoint_id: &str,
        entity_id: &str,
    ) -> Result<(), CheckpointError> {
        save_file_snapshot(
            self.file_checkpoint_manager.as_ref(),
            checkpoint_id,
            entity_id,
        )
        .await
    }

    async fn enqueue_persistence(&self, checkpoint_id: &str, entity_id: &str) {
        enqueue_persistence(
            &self.persistence_queue,
            self.file_checkpoint_manager.as_ref(),
            self.event_bus.as_ref(),
            checkpoint_id,
            entity_id,
        )
        .await;
    }

    /// Drain the persistence queue and wait for all deferred operations.
    async fn wait_for_persistence(&self) {
        let queue_metrics = self
            .file_checkpoint_manager
            .as_ref()
            .and_then(|m| m.checkpoint_metrics_for_observability());
        drain_persistence_handles(
            &self.persistence_queue,
            self.event_bus.as_ref(),
            "all",
            queue_metrics.as_deref(),
        )
        .await;
    }

    async fn prepare(
        &self,
        entity_id: &str,
        trigger: CheckpointTiming,
    ) -> Result<CheckpointContext, CheckpointError> {
        self.prepare_with_parent(entity_id, trigger, None).await
    }

    async fn build(
        &self,
        ctx: CheckpointContext,
        mut state: Self::State,
    ) -> Result<Self::Checkpoint, CheckpointError> {
        // Progress coordinates describe the execution state, not the stored
        // payload: compute them before the content policy may strip blob
        // domains, so filtered checkpoints still dedup identically.
        let coords = snapshot_workflow_coords(&state);
        // Content policy (ContentFilter) applied before any storage type
        // decision is made.
        self.apply_content_policy(&mut state)?;

        let previous = self.state_manager.get_latest(&ctx.entity_id).await?;

        let checkpoint_type = self
            .determine_type(&ctx.entity_id, &self.delta_config)
            .await?;

        // Metadata: trigger description/tag, caller custom fields (e.g.
        // node/tool ids), plus the injected formatVersion/createdAt/
        // chainPosition. The wire shape is a flat map with
        // description/tags/customFields keys.
        let chain_position: u32 = next_chain_position(
            &checkpoint_type,
            previous.as_ref().and_then(|p| p.chain_position),
        );
        let custom_fields = progress_custom_fields(&coords, chain_position);
        let custom_fields = stamp_custom_fields(ctx.metadata, custom_fields);
        let now_ms = self.clock.now_ms().ok_or_else(|| {
            CheckpointError::Internal(
                "checkpoint clock unavailable; refusing to stamp a checkpoint".to_string(),
            )
        })?;
        let metadata = build_checkpoint_metadata(
            ctx.trigger.as_ref().map(trigger_description),
            ctx.trigger.as_ref().map(trigger_tag).into_iter().collect(),
            custom_fields,
            self.version_manager.current_version(),
            now_ms,
        );

        match checkpoint_type {
            CheckpointType::Full => Ok(BaseCheckpointCore {
                id: wf_common::generate_id(),
                r#type: Some(CheckpointType::Full),
                base_checkpoint_id: None,
                previous_checkpoint_id: previous.map(|p| p.id),
                delta: None,
                snapshot: Some(state),
                timestamp: Some(now_ms),
                metadata,
                format_version: Some(self.version_manager.current_version().to_string()),
            }),
            CheckpointType::Delta => {
                // Diff against the nearest checkpoint that still carries a
                // full snapshot (the chain base); deltas in between have no
                // snapshot of their own. If no base can be established the
                // delta would be unrestorable, so fall back to a FULL
                // checkpoint instead.
                let (base_id, base_snapshot) = self.find_base(&previous).await?;

                match base_snapshot {
                    Some(base_snapshot) => {
                        let delta = self
                            .diff_calculator
                            .calculate_diff(&base_snapshot, &state)
                            .await?;

                        Ok(BaseCheckpointCore {
                            id: wf_common::generate_id(),
                            r#type: Some(CheckpointType::Delta),
                            base_checkpoint_id: base_id,
                            previous_checkpoint_id: previous.map(|p| p.id),
                            delta: Some(delta),
                            snapshot: None,
                            timestamp: Some(now_ms),
                            metadata,
                            format_version: Some(
                                self.version_manager.current_version().to_string(),
                            ),
                        })
                    }
                    None => Ok(BaseCheckpointCore {
                        id: wf_common::generate_id(),
                        r#type: Some(CheckpointType::Full),
                        base_checkpoint_id: None,
                        previous_checkpoint_id: previous.map(|p| p.id),
                        delta: None,
                        snapshot: Some(state),
                        timestamp: Some(now_ms),
                        metadata,
                        format_version: Some(self.version_manager.current_version().to_string()),
                    }),
                }
            }
        }
    }

    async fn persist(
        &self,
        checkpoint: &Self::Checkpoint,
        entity_id: &str,
    ) -> Result<(), CheckpointError> {
        persist_checkpoint(
            &self.state_manager,
            checkpoint,
            "workflow_execution",
            entity_id,
            self.event_bus.as_ref(),
            &self.error_handler,
        )
        .await?;
        // Retention enforcement is best-effort and never fails the persist
        // that already succeeded.
        if let Some(ref strategy) = self.strategy {
            let now = self.state_manager.clock().now_ms();
            crate::coordinator::base::enforce_retention(
                &self.state_manager,
                strategy,
                entity_id,
                now,
            )
            .await;
        }
        Ok(())
    }

    async fn validate_checkpoint(
        &self,
        checkpoint: &Self::Checkpoint,
    ) -> Result<(), CheckpointError> {
        if checkpoint.id.is_empty() {
            return Err(CheckpointError::Validation {
                reason: "checkpoint id is empty".to_string(),
            });
        }
        match &checkpoint.r#type {
            Some(CheckpointType::Full) => match &checkpoint.snapshot {
                Some(snapshot) if !snapshot.execution_id.is_empty() => Ok(()),
                Some(_) => Err(CheckpointError::Validation {
                    reason: "full checkpoint missing execution_id".to_string(),
                }),
                None => Err(CheckpointError::Validation {
                    reason: "full checkpoint missing snapshot".to_string(),
                }),
            },
            Some(CheckpointType::Delta) => {
                if checkpoint.base_checkpoint_id.is_none() {
                    return Err(CheckpointError::Validation {
                        reason: "delta checkpoint missing base_checkpoint_id".to_string(),
                    });
                }
                if checkpoint.previous_checkpoint_id.is_none() {
                    return Err(CheckpointError::Validation {
                        reason: "delta checkpoint missing previous_checkpoint_id".to_string(),
                    });
                }
                if checkpoint.delta.is_none() {
                    return Err(CheckpointError::Validation {
                        reason: "delta checkpoint missing delta".to_string(),
                    });
                }
                Ok(())
            }
            None => Err(CheckpointError::Validation {
                reason: "checkpoint has no type".to_string(),
            }),
        }
    }

    async fn restore(&self, checkpoint_id: &str) -> Result<Self::Entity, CheckpointError> {
        let checkpoint = self.load_migrated(checkpoint_id).await?;
        self.validate_checkpoint(&checkpoint).await?;

        let mut entity = match checkpoint.r#type {
            Some(CheckpointType::Full) => {
                let snapshot = checkpoint
                    .snapshot
                    .ok_or_else(|| CheckpointError::Corrupted {
                        id: checkpoint_id.to_string(),
                        reason: "full checkpoint missing snapshot".to_string(),
                    })?;

                Ok(WorkflowExecutionEntity {
                    execution_id: snapshot.execution_id.clone(),
                    status: snapshot.status.clone(),
                    snapshot,
                    restore_summary: None,
                })
            }
            Some(CheckpointType::Delta) => {
                let restorer = GenericDeltaRestorer::new(self.diff_calculator.clone());
                let state = restorer
                    .restore_full_state(checkpoint_id, &self.state_manager)
                    .await?;

                Ok(WorkflowExecutionEntity {
                    execution_id: state.execution_id.clone(),
                    status: state.status.clone(),
                    snapshot: state,
                    restore_summary: None,
                })
            }
            None => Err(CheckpointError::Corrupted {
                id: checkpoint_id.to_string(),
                reason: "checkpoint has no type".to_string(),
            }),
        }?;

        // Post-restore phase: bring back the child executions spawned from
        // this one. They are located through their own records rather than a
        // list cached in this snapshot.
        if let Ok(summary) = self
            .restore_child_hierarchy(checkpoint_id, &entity.execution_id)
            .await
        {
            entity.restore_summary = Some(summary);
        }

        // restore the file set linked to this state checkpoint (falls back
        // to the latest file checkpoint for pre-link history). Best-effort:
        // state restore stands regardless of file history.
        restore_state_files(
            self.file_checkpoint_manager.as_ref(),
            &entity.execution_id,
            checkpoint_id,
        );

        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::restored(
                checkpoint_id.to_string(),
                entity.execution_id.clone(),
            ));
        }

        Ok(entity)
    }

    async fn delete(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        delete_checkpoint(&self.state_manager, checkpoint_id, self.event_bus.as_ref()).await
    }

    async fn determine_type(
        &self,
        entity_id: &str,
        config: &DeltaStorageConfig,
    ) -> Result<CheckpointType, CheckpointError> {
        // aggregate COUNT query instead of materializing the full
        // history listing.
        let count = self.state_manager.count_by_entity(entity_id).await?;
        Ok(decide_checkpoint_type_by_count(count, config))
    }

    fn default_strategy(&self) -> Option<&dyn CheckpointStrategy> {
        self.strategy.as_ref().map(|s| s as &dyn CheckpointStrategy)
    }

    fn event_bus(&self) -> Option<&CheckpointEventBus> {
        self.event_bus.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowExecutionEntity {
    pub execution_id: String,
    pub status: String,
    pub snapshot: WorkflowExecutionStateSnapshot,
    pub restore_summary: Option<ChildDiscoverySummary>,
}

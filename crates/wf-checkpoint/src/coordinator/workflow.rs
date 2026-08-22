use crate::content::SizeBudget;
use crate::coordinator::CheckpointCoordinator;
use crate::delta::CheckpointLoader;
use crate::delta::DeltaRestorer;
use crate::delta::DiffCalculator;
use crate::delta::GenericDeltaRestorer;
use crate::delta::WorkflowDiffCalculator;
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::file::FileCheckpointManager;
use crate::metadata_builder::{
    build_checkpoint_metadata, trigger_description, trigger_tag, CHAIN_POSITION_FIELD,
};
use crate::restore::fork_join::{ForkJoinStateInference, JoinStateInference};
use crate::restore::hierarchy::{HierarchyRestorer, RestoreSummary, StorageChildResolver};
use crate::restore::integrity::{
    ExecutionRegistry, HierarchyIntegrityService, HierarchyValidationResult,
};
use crate::restore::registry::RestoreStrategyRegistry;
use crate::serializer::CheckpointSerializer;
use crate::state::CheckpointStateManager;
use crate::state::WorkflowCheckpoint;
use crate::state::WorkflowCheckpointStateManager;
use crate::strategy::CheckpointStrategy;
use crate::strategy::StandardStrategy;
use crate::version::VersionManager;
use crate::version::MIN_COMPATIBLE_VERSION;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use wf_common::gate::ConcurrencyGate;
use wf_types::checkpoint::workflow::WorkflowCheckpointDelta;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::CheckpointContext;
use wf_types::checkpoint::CheckpointTiming;
use wf_types::checkpoint::CheckpointType;
use wf_types::checkpoint::DeltaStorageConfig;
use wf_types::checkpoint::UnifiedCheckpointPolicy;
use wf_types::execution::ExecutionStatus;
use wf_types::message::Message;
use wf_types::storage::CheckpointStorageMetadata;

/// Upper bound on the deferred persistence queue; when reached the oldest
/// deferred work is awaited before enqueueing so memory stays bounded.
const MAX_PERSISTENCE_QUEUE: usize = 128;

pub struct WorkflowCheckpointCoordinator {
    state_manager: WorkflowCheckpointStateManager,
    diff_calculator:
        Arc<dyn DiffCalculator<WorkflowExecutionStateSnapshot, WorkflowCheckpointDelta>>,
    event_bus: Option<CheckpointEventBus>,
    delta_config: DeltaStorageConfig,
    version_manager: VersionManager,
    strategy: Option<StandardStrategy>,
    cadence: HashMap<CheckpointTiming, u32>,
    cadence_attempts: dashmap::DashMap<String, u32>,
    error_handler: crate::error_handling::CheckpointErrorHandler,
    size_budget: Option<SizeBudget>,
    restore_registry: Option<RestoreStrategyRegistry>,
    execution_registry: Option<Arc<dyn ExecutionRegistry>>,
    file_checkpoint_manager: Option<FileCheckpointManager>,
    /// `contentConfig.async`: defer post-persist side effects to the
    /// background persistence queue.
    async_persistence: bool,
    /// Background persistence queue (`persistenceQueue`); drained by
    /// `wait_for_persistence`.
    persistence_queue: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl WorkflowCheckpointCoordinator {
    pub fn new(state_manager: WorkflowCheckpointStateManager) -> Self {
        Self {
            state_manager,
            diff_calculator: Arc::new(WorkflowDiffCalculator::new()),
            event_bus: None,
            delta_config: DeltaStorageConfig::default(),
            version_manager: VersionManager::new(),
            strategy: None,
            cadence: HashMap::new(),
            cadence_attempts: dashmap::DashMap::new(),
            error_handler: crate::error_handling::CheckpointErrorHandler::default(),
            size_budget: None,
            restore_registry: None,
            execution_registry: None,
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

    /// Configure the default checkpoint strategy from a unified policy.
    /// A disabled policy yields a strategy that never checkpoints. The
    /// policy's `content.async` flag also enables async persistence mode.
    pub fn with_strategy(mut self, policy: &UnifiedCheckpointPolicy) -> Self {
        self.strategy = Some(crate::strategy::create_checkpoint_strategy(policy));
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

    /// Set an interval cadence for a trigger: in the strategy-gated create
    /// path the checkpoint fires only every `n` existing checkpoints.
    /// May be called multiple times.
    pub fn with_cadence(mut self, trigger: CheckpointTiming, n: u32) -> Self {
        self.cadence.insert(trigger, n.max(1));
        self
    }

    /// Configure the checkpoint error handler (default: `warn`, non-fatal).
    pub fn with_error_handler(
        mut self,
        handler: crate::error_handling::CheckpointErrorHandler,
    ) -> Self {
        self.error_handler = handler;
        self
    }

    /// Configure the error handler from a unified policy.
    pub fn with_error_policy(mut self, policy: &UnifiedCheckpointPolicy) -> Self {
        self.error_handler = crate::error_handling::CheckpointErrorHandler::from_policy(policy);
        self
    }

    pub fn with_size_budget(mut self, budget: SizeBudget) -> Self {
        self.size_budget = Some(budget);
        self
    }

    /// Register restore strategies used for child execution recovery in the
    /// post-restore phase.
    pub fn with_restore_registry(mut self, registry: RestoreStrategyRegistry) -> Self {
        self.restore_registry = Some(registry);
        self
    }

    /// Register the execution registry used for hierarchy integrity
    /// validation and FORK/JOIN status inference after restore.
    pub fn with_execution_registry(mut self, registry: Arc<dyn ExecutionRegistry>) -> Self {
        self.execution_registry = Some(registry);
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

    fn apply_content_policy(&self, state: &mut WorkflowExecutionStateSnapshot) {
        if let Some(strategy) = &self.strategy {
            let filter = crate::content::ContentFilter::new();
            let config = strategy.content_config();
            if !filter.should_include_state(config) {
                state.input = None;
                state.output = None;
                state.node_results = None;
                state.messages = None;
                state.fork_join_context = None;
                state.active_operations = None;
                state.error_records = None;
                state.interruption_records = None;
                state.event_records = None;
                state.fork_join_aggregation_state = None;
                state.hook_execution_context = None;
                state.execution_config = None;
                state.conversation_state = None;
                state.trigger_states = None;
            }
            if !filter.should_include_history(config) {
                state.messages = None;
            }
        }
        if let Some(budget) = &self.size_budget {
            // Progressive size-budget truncation; any dropped content is
            // recorded on the snapshot (`truncated` + `truncation_stats`)
            // so restore can warn about the degraded state.
            budget.truncate_snapshot(state);
        }
    }

    /// When a size budget is configured and the snapshot still exceeds it
    /// after truncation, degrade the storage type to FULL: a compact delta
    /// chain cannot help when the snapshot itself is the payload.
    fn snapshot_over_budget(&self, state: &WorkflowExecutionStateSnapshot) -> bool {
        match &self.size_budget {
            Some(budget) => serde_json::to_vec(state)
                .map(|bytes| !budget.is_within_budget(bytes.len()))
                .unwrap_or(false),
            None => false,
        }
    }

    /// Load the checkpoint blob and bring it to the current format version.
    async fn load_migrated(
        &self,
        checkpoint_id: &str,
    ) -> Result<WorkflowCheckpoint, CheckpointError> {
        let checkpoint = self
            .state_manager
            .load(checkpoint_id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            })?;

        let version = checkpoint
            .format_version
            .as_deref()
            .unwrap_or(MIN_COMPATIBLE_VERSION);

        let compatibility = self.version_manager.check_compatibility(version);
        if !compatibility.compatible {
            return Err(CheckpointError::VersionIncompatible {
                current: self.version_manager.current_version().to_string(),
                required: version.to_string(),
            });
        }

        if !compatibility.requires_migration {
            return Ok(checkpoint);
        }

        // Re-read the raw bytes so the migration can rewrite the blob.
        let raw = self
            .state_manager
            .load_checkpoint_data(checkpoint_id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            })?;
        let migrated = self.version_manager.migrate_data(&raw, version).await?;
        CheckpointSerializer::auto_deserialize(&migrated)
    }

    /// Rebuild the truncated message history by walking the checkpoint chain
    /// from the base checkpoint up to the target's predecessor, merging
    /// messages in order. The target checkpoint itself is skipped: its
    /// snapshot carries no messages (that is why the rebuild runs), so its
    /// own delta's message ops are diff artifacts of the omitted field, not
    /// real history. Only runs when `messages` is absent and
    /// `message_base_checkpoint_id` is set (i.e. the snapshot was created
    /// with the message chain link).
    async fn rebuild_message_chain(
        &self,
        checkpoint_id: &str,
        state: &mut WorkflowExecutionStateSnapshot,
    ) -> Result<(), CheckpointError> {
        if let Some(messages) = &state.messages {
            if !messages.is_empty() {
                return Ok(());
            }
        }
        let Some(base_id) = state.message_base_checkpoint_id.clone() else {
            return Ok(());
        };

        // Start the walk from the target's predecessor (see doc comment).
        let target_meta = self
            .state_manager
            .load_metadata(checkpoint_id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            })?;

        let mut chain_ids: Vec<String> = Vec::new();
        let mut cursor = target_meta.previous_checkpoint_id;
        let mut visited = HashSet::new();
        while let Some(id) = cursor {
            if !visited.insert(id.clone()) {
                return Err(CheckpointError::Corrupted {
                    id: id.clone(),
                    reason: "circular reference in message chain".to_string(),
                });
            }
            chain_ids.push(id.clone());
            if id == base_id {
                break;
            }
            let meta = self
                .state_manager
                .load_metadata(&id)
                .await?
                .ok_or_else(|| CheckpointError::NotFound { id: id.clone() })?;
            cursor = meta.previous_checkpoint_id;
        }
        chain_ids.reverse();

        let mut messages: Vec<Message> = Vec::new();
        for id in chain_ids {
            let checkpoint = self
                .state_manager
                .load(&id)
                .await?
                .ok_or_else(|| CheckpointError::NotFound { id: id.clone() })?;
            match checkpoint.r#type {
                Some(CheckpointType::Full) => {
                    if let Some(snapshot) = checkpoint.snapshot {
                        if let Some(snap_messages) = snapshot.messages {
                            messages = snap_messages;
                        }
                    }
                }
                Some(CheckpointType::Delta) => {
                    if let Some(delta) = checkpoint.delta {
                        if let Some(added) = delta.added_messages {
                            messages.extend(added);
                        }
                        if let Some(modified) = delta.modified_messages {
                            for message in modified {
                                if let Some(idx) = messages.iter().position(|m| m.id == message.id)
                                {
                                    messages[idx] = message;
                                }
                            }
                        }
                        if let Some(deleted) = delta.deleted_message_indices {
                            let mut indices: Vec<usize> =
                                deleted.iter().map(|idx| *idx as usize).collect();
                            indices.sort_unstable_by(|a, b| b.cmp(a));
                            for idx in indices {
                                if idx < messages.len() {
                                    messages.remove(idx);
                                }
                            }
                        }
                    }
                }
                None => {}
            }
        }

        if let Some(total) = state.message_total_count {
            let total = total as usize;
            if messages.len() > total {
                messages = messages.split_off(messages.len() - total);
            }
        }
        state.messages = Some(messages);
        Ok(())
    }

    /// Post-restore phase: restore child executions through the hierarchy
    /// metadata. Latest checkpoints of child executions are resolved from
    /// storage with bounded concurrency, BFS-restored via `HierarchyRestorer`,
    /// and (when a restore strategy is registered for the child execution
    /// type) fully restored through the strategy registry. Restored children
    /// are registered into the execution registry for integrity validation
    /// and JOIN inference. Children that could not be restored are returned
    /// so the caller can remove them from the hierarchy metadata.
    async fn restore_child_hierarchy(
        &self,
        checkpoint_id: &str,
        parent_entity_id: &str,
        hierarchy: &wf_types::execution::ExecutionHierarchy,
        registry: Option<&Arc<dyn ExecutionRegistry>>,
    ) -> Result<(RestoreSummary, Vec<String>), CheckpointError> {
        let mut children = hierarchy.children.clone().unwrap_or_default();
        if children.is_empty() {
            return Ok((
                RestoreSummary {
                    total: 0,
                    success: 0,
                    failed: 0,
                },
                Vec::new(),
            ));
        }

        // WORKFLOW children restore before AGENT_LOOP children.
        children.sort_by_key(|c| match c.child_type {
            wf_types::execution::ExecutionType::Workflow => 0,
            wf_types::execution::ExecutionType::AgentLoop => 1,
        });

        // Resolve the latest checkpoint for every child in a single storage
        // query (IN filter) instead of one `get_latest` per child.
        let child_ids: Vec<String> = children.iter().map(|c| c.child_id.clone()).collect();
        let latest_by_child: HashMap<String, CheckpointStorageMetadata> = self
            .state_manager
            .list_latest_by_entities(&child_ids)
            .await?
            .into_iter()
            .map(|meta| (meta.entity_id.clone(), meta))
            .collect();

        // Bounded concurrency for the per-child restore phase.
        let gate = Arc::new(
            ConcurrencyGate::new(CHILD_RESTORE_CONCURRENCY),
        );
        let storage = self.state_manager.storage().clone();
        let restore_registry = self.restore_registry.clone();
        let mut handles = Vec::new();
        for child in &children {
            let Some(meta) = latest_by_child.get(&child.child_id) else {
                tracing::debug!(
                    parent = %parent_entity_id,
                    child = %child.child_id,
                    "child has no checkpoint; skipped"
                );
                continue;
            };
            let gate = gate.clone();
            let child = child.clone();
            let meta = meta.clone();
            let parent_entity_id = parent_entity_id.to_string();
            let registry = registry.cloned();
            let storage = storage.clone();
            let restore_registry = restore_registry.clone();
            handles.push(tokio::spawn(async move {
                let _permit = match gate.acquire_wait().await {
                    Ok(permit) => permit,
                    Err(e) => {
                        return Err(CheckpointError::Internal(format!(
                            "child restore gate acquire failed: {e}"
                        )))
                    }
                };
                let state_manager = WorkflowCheckpointStateManager::new(storage);
                restore_child(
                    &state_manager,
                    restore_registry.as_ref(),
                    &child,
                    &parent_entity_id,
                    registry.as_deref(),
                    meta,
                )
                .await
            }));
        }

        let resolver = StorageChildResolver::new();
        let mut index: HashMap<String, CheckpointStorageMetadata> = HashMap::new();
        let mut failed_children = Vec::new();
        let mut restored = 0u32;

        for handle in handles {
            match handle.await {
                Ok(Ok(outcome)) => {
                    if let Some(meta) = outcome.metadata {
                        index.insert(meta.id.clone(), meta.clone());
                        resolver.register_relationship(checkpoint_id, &meta.id);
                        if outcome.restored {
                            restored += 1;
                        }
                    }
                    if outcome.failed {
                        failed_children.push(outcome.child_id);
                    }
                }
                Ok(Err(_)) => {
                    // resolution/restore error: treat the child as failed.
                }
                Err(join_err) => {
                    tracing::warn!(
                        parent = %parent_entity_id,
                        error = %join_err,
                        "child restore task panicked"
                    );
                }
            }
        }

        let loader = MetadataIndexLoader::new(index);
        let restorer = HierarchyRestorer::new(Arc::new(resolver));
        let results = restorer.restore_children_bfs(checkpoint_id, &loader, 8, None)?;
        let mut summary = HierarchyRestorer::summarize_results(&results);
        summary.success += restored as usize;
        Ok((summary, failed_children))
    }

    /// Post-restore FORK/JOIN inference: when the restored snapshot was
    /// captured at a JOIN node, infer per-path completion status from the
    /// restored child executions in the registry.
    fn infer_join_state(
        &self,
        snapshot: &WorkflowExecutionStateSnapshot,
        registry: &dyn ExecutionRegistry,
    ) -> Option<JoinStateInference> {
        let aggregation = snapshot.fork_join_aggregation_state.as_ref()?;
        let path_statuses = aggregation.get("pathStatuses")?;
        let path_ids: Vec<String> = match path_statuses {
            serde_json::Value::Object(map) => map.keys().cloned().collect(),
            _ => Vec::new(),
        };
        if path_ids.is_empty() {
            return None;
        }
        let hierarchy = snapshot.hierarchy.as_ref();
        Some(ForkJoinStateInference::infer(
            &path_ids,
            &snapshot.execution_id,
            hierarchy,
            registry,
        ))
    }
}

fn parse_execution_status(value: &serde_json::Value) -> Option<ExecutionStatus> {
    let status = value.get("status")?.as_str()?;
    Some(parse_status_string(status))
}

/// Restore a single child from its pre-resolved latest checkpoint metadata
/// through the restore strategy registry when one is registered for the
/// child's execution type. Spawned with bounded concurrency by
/// `restore_child_hierarchy`.
async fn restore_child(
    state_manager: &WorkflowCheckpointStateManager,
    restore_registry: Option<&RestoreStrategyRegistry>,
    child: &wf_types::execution::ChildExecutionReference,
    parent_entity_id: &str,
    registry: Option<&dyn ExecutionRegistry>,
    meta: CheckpointStorageMetadata,
) -> Result<ChildRestoreOutcome, CheckpointError> {
    let mut outcome = ChildRestoreOutcome {
        child_id: child.child_id.clone(),
        metadata: Some(meta.clone()),
        restored: false,
        failed: false,
    };

    if let Some(reg) = restore_registry {
        let entity_type = match child.child_type {
            wf_types::execution::ExecutionType::Workflow => "workflow_execution",
            wf_types::execution::ExecutionType::AgentLoop => "agent_loop",
        };
        if let Some(data) = state_manager.load_checkpoint_data(&meta.id).await? {
            let restore_result = reg.restore(entity_type, &meta.id, &data).await;
            if let Ok(value) = restore_result {
                outcome.restored = true;
                if let Some(exec_registry) = registry {
                    let status = parse_execution_status(&value);
                    register_child(
                        exec_registry,
                        &child.child_id,
                        status,
                        Some(parent_entity_id),
                        child.fork_path_id.as_deref(),
                    );
                }
            } else {
                outcome.failed = true;
            }
        } else {
            outcome.failed = true;
        }
    }
    Ok(outcome)
}

/// Bounded concurrency for the child restore phase.
const CHILD_RESTORE_CONCURRENCY: usize = 5;

struct ChildRestoreOutcome {
    child_id: String,
    metadata: Option<CheckpointStorageMetadata>,
    restored: bool,
    failed: bool,
}

fn parse_status_string(status: &str) -> ExecutionStatus {
    match status {
        "completed" => ExecutionStatus::Completed,
        "failed" => ExecutionStatus::Failed,
        "cancelled" => ExecutionStatus::Cancelled,
        "paused" => ExecutionStatus::Paused,
        "stopped" => ExecutionStatus::Stopped,
        "created" => ExecutionStatus::Created,
        _ => ExecutionStatus::Running,
    }
}

fn register_child(
    registry: &dyn ExecutionRegistry,
    child_id: &str,
    status: Option<ExecutionStatus>,
    parent: Option<&str>,
    fork_path_id: Option<&str>,
) {
    let status = status.unwrap_or(ExecutionStatus::Running);
    match (parent, fork_path_id) {
        (Some(parent), Some(path)) => {
            registry.register_fork_path(child_id, status, parent, path);
        }
        (Some(parent), None) => {
            registry.register_with_parent(child_id, status, Some(parent));
        }
        _ => registry.register(child_id, status),
    }
}

impl CheckpointCoordinator for WorkflowCheckpointCoordinator {
    type Checkpoint = WorkflowCheckpoint;
    type Entity = WorkflowExecutionEntity;
    type State = WorkflowExecutionStateSnapshot;

    fn async_persistence_enabled(&self) -> bool {
        self.async_persistence
    }

    /// Defer post-persist side effects (file snapshot) to the background
    /// persistence queue (async mode). The queue is bounded:
    /// when it exceeds `MAX_PERSISTENCE_QUEUE`, the oldest deferred work is
    /// awaited first so the `Vec<JoinHandle>` never grows without bound.
    async fn enqueue_persistence(&self, checkpoint_id: &str, entity_id: &str) {
        let checkpoint_id = checkpoint_id.to_string();
        let entity_id = entity_id.to_string();
        let file_manager = self.file_checkpoint_manager.clone();
        let queue = self.persistence_queue.clone();
        // File-snapshot creation is blocking file I/O; run it on the blocking
        // pool so a slow filesystem never pins a tokio worker.
        let handle = tokio::task::spawn_blocking(move || {
            if let Some(manager) = file_manager {
                if let Err(err) = manager.create_latest_file_checkpoint(&entity_id) {
                    tracing::warn!(
                        entity_id = %entity_id,
                        checkpoint_id = %checkpoint_id,
                        error = %err,
                        "deferred file checkpoint creation failed (best-effort)"
                    );
                }
            }
        });
        let mut queue_guard = queue.lock().await;
        if queue_guard.len() >= MAX_PERSISTENCE_QUEUE {
            let backlog: Vec<_> = std::mem::take(&mut *queue_guard);
            drop(queue_guard);
            for task in backlog {
                if let Err(join_err) = task.await {
                    tracing::warn!(error = %join_err, "persistence task panicked");
                }
            }
            queue_guard = queue.lock().await;
        }
        queue_guard.push(handle);
    }

    /// Drain the persistence queue and wait for all deferred operations.
    async fn wait_for_persistence(&self) {
        let handles: Vec<_> = {
            let mut queue = self.persistence_queue.lock().await;
            std::mem::take(&mut *queue)
        };
        for handle in handles {
            if let Err(join_err) = handle.await {
                tracing::warn!(error = %join_err, "persistence task panicked");
            }
        }
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
        // Content policy (ContentFilter + SizeBudget) applied before any
        // storage type decision is made.
        self.apply_content_policy(&mut state);

        let previous = self.state_manager.get_latest(&ctx.entity_id).await?;

        let mut checkpoint_type = self
            .determine_type(&ctx.entity_id, &self.delta_config)
            .await?;
        if checkpoint_type == CheckpointType::Delta && self.snapshot_over_budget(&state) {
            checkpoint_type = CheckpointType::Full;
        }

        // Metadata: trigger description/tag, caller custom fields (e.g.
        // node/tool ids), plus the injected formatVersion/createdAt/
        // chainPosition. The wire shape is a flat map with
        // description/tags/customFields keys.
        let chain_position: u32 = match checkpoint_type {
            CheckpointType::Full => 0,
            CheckpointType::Delta => previous
                .as_ref()
                .and_then(|p| p.chain_position)
                .map(|p| p + 1)
                .unwrap_or(1),
        };
        let mut custom_fields = ctx.metadata.clone().unwrap_or_default();
        custom_fields.insert(
            CHAIN_POSITION_FIELD.to_string(),
            serde_json::json!(chain_position),
        );
        let metadata = build_checkpoint_metadata(
            ctx.trigger.as_ref().map(trigger_description),
            ctx.trigger.as_ref().map(trigger_tag).into_iter().collect(),
            custom_fields,
            self.version_manager.current_version(),
        );

        match checkpoint_type {
            CheckpointType::Full => Ok(BaseCheckpointCore {
                id: wf_common::generate_id(),
                r#type: Some(CheckpointType::Full),
                base_checkpoint_id: None,
                previous_checkpoint_id: previous.map(|p| p.id),
                delta: None,
                snapshot: Some(state),
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
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
                            timestamp: Some(chrono::Utc::now().timestamp_millis()),
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
                        timestamp: Some(chrono::Utc::now().timestamp_millis()),
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
        if let Err(err) = self
            .state_manager
            .save(checkpoint, "workflow_execution", entity_id)
            .await
        {
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::failed_with(
                    Some(checkpoint.id.clone()),
                    "create",
                    format!("persist failed: {}", err),
                    Some(entity_id.to_string()),
                ));
            }
            // Route through the checkpoint error handler: non-fatal
            // strategies (warn/silent) swallow the failure so the execution
            // continues without a checkpoint.
            let context =
                self.error_handler
                    .context("create", Some(checkpoint.id.clone()), None, 0);
            let outcome = self.error_handler.decide(&context, &err);
            if outcome.should_rethrow {
                return Err(err);
            }
            return Ok(());
        }

        if let Some(ref bus) = self.event_bus {
            let description = checkpoint
                .metadata
                .as_ref()
                .and_then(|m| m.get("description"))
                .and_then(|v| v.as_str());
            bus.publish(CheckpointEventBus::created_with(
                checkpoint.id.clone(),
                Some(entity_id.to_string()),
                description.map(String::from),
            ));
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
                    hierarchy_validation: None,
                    join_inference: None,
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
                    hierarchy_validation: None,
                    join_inference: None,
                })
            }
            None => Err(CheckpointError::Corrupted {
                id: checkpoint_id.to_string(),
                reason: "checkpoint has no type".to_string(),
            }),
        }?;

        // rebuild truncated message history through the message chain.
        self.rebuild_message_chain(checkpoint_id, &mut entity.snapshot)
            .await?;

        // a snapshot truncated by the size budget resumes degraded —
        // surface the drop statistics so consumers know the state is lossy.
        if entity.snapshot.truncated == Some(true) {
            tracing::warn!(
                target: "wf_checkpoint",
                execution_id = %entity.snapshot.execution_id,
                checkpoint_id,
                truncation = ?entity.snapshot.truncation_stats,
                "Restored snapshot was truncated by the checkpoint size budget; state may be incomplete"
            );
        }

        // register the restored entity into the execution registry,
        // restore child executions, then validate hierarchy integrity and
        // infer FORK/JOIN completion status.
        let mut validation: Option<HierarchyValidationResult> = None;
        let mut join_inference: Option<JoinStateInference> = None;
        let mut failed_child_ids: Vec<String> = Vec::new();
        if let Some(registry) = &self.execution_registry {
            let hierarchy = entity.snapshot.hierarchy.clone();
            let parent = hierarchy
                .as_ref()
                .and_then(|h| h.parent_execution_id.clone());
            registry.register_with_parent(
                &entity.execution_id,
                parse_status_string(&entity.status),
                parent.as_deref(),
            );

            // post-restore phase — restore child executions from hierarchy.
            if let Some(h) = &hierarchy {
                if let Ok((summary, failed)) = self
                    .restore_child_hierarchy(checkpoint_id, &entity.execution_id, h, Some(registry))
                    .await
                {
                    entity.restore_summary = Some(summary);
                    failed_child_ids = failed;
                }
                validation = Some(HierarchyIntegrityService::validate_integrity(
                    h,
                    registry.as_ref(),
                ));
            }

            // FORK/JOIN status inference after child restoration.
            if let Some(inferred) = self.infer_join_state(&entity.snapshot, registry.as_ref()) {
                join_inference = Some(inferred);
            }
        } else if let Some(hierarchy) = entity.snapshot.hierarchy.clone() {
            // fallback: restore children without a registry.
            if let Ok((summary, failed)) = self
                .restore_child_hierarchy(checkpoint_id, &entity.execution_id, &hierarchy, None)
                .await
            {
                entity.restore_summary = Some(summary);
                failed_child_ids = failed;
            }
        }
        entity.hierarchy_validation = validation;
        entity.join_inference = join_inference;

        // Remove children that could not be restored from the restored
        // entity's hierarchy metadata.
        if !failed_child_ids.is_empty() {
            if let Some(hierarchy) = &mut entity.snapshot.hierarchy {
                if let Some(children) = &mut hierarchy.children {
                    children.retain(|c| !failed_child_ids.contains(&c.child_id));
                }
            }
        }

        // restore the latest file checkpoint for the entity (best-effort).
        if let Some(manager) = &self.file_checkpoint_manager {
            if let Err(err) = manager.restore_latest(&entity.execution_id) {
                tracing::warn!(
                    "file checkpoint restore failed for entity {}: {}",
                    entity.execution_id,
                    err
                );
            }
        }

        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::restored(
                checkpoint_id.to_string(),
                entity.execution_id.clone(),
            ));
        }

        Ok(entity)
    }

    async fn delete(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        let deleted = self.state_manager.delete(checkpoint_id).await?;
        if deleted {
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::deleted_with(
                    checkpoint_id.to_string(),
                    Some("manual".to_string()),
                ));
            }
        }
        Ok(deleted)
    }

    async fn determine_type(
        &self,
        entity_id: &str,
        config: &DeltaStorageConfig,
    ) -> Result<CheckpointType, CheckpointError> {
        if !config.enabled {
            return Ok(CheckpointType::Full);
        }

        // aggregate COUNT query instead of materializing the full
        // history listing.
        let count = self.state_manager.count_by_entity(entity_id).await? as u32;
        let effective_interval = config.baseline_interval.min(config.max_delta_chain_length);

        if count == 0 || effective_interval == 0 || count.is_multiple_of(effective_interval) {
            return Ok(CheckpointType::Full);
        }

        Ok(CheckpointType::Delta)
    }

    fn default_strategy(&self) -> Option<&dyn CheckpointStrategy> {
        self.strategy.as_ref().map(|s| s as &dyn CheckpointStrategy)
    }

    /// Strategy-gated create with the optional per-trigger cadence: the
    /// checkpoint fires only every `n` attempts for the entity (attempts
    /// increment regardless of whether the checkpoint is created, matching
    /// the agent-loop iteration cadence semantics).
    async fn create_checkpoint_with_strategy(
        &self,
        trigger: CheckpointTiming,
        entity_id: &str,
        state: Self::State,
    ) -> Result<Option<String>, CheckpointError> {
        let ctx = self.prepare(entity_id, trigger.clone()).await?;
        if let Some(strategy) = self.default_strategy() {
            if !strategy.should_checkpoint(&trigger, &ctx) {
                return Ok(None);
            }
        }
        if let Some(cadence) = self.cadence.get(&trigger) {
            if *cadence > 1 {
                let attempt = {
                    let mut entry = self
                        .cadence_attempts
                        .entry(entity_id.to_string())
                        .or_insert(0);
                    *entry += 1;
                    *entry
                };
                if !attempt.is_multiple_of(*cadence) {
                    return Ok(None);
                }
            }
        }
        let id = self.create_checkpoint(trigger, entity_id, state).await?;
        Ok(Some(id))
    }
}

impl WorkflowCheckpointCoordinator {
    /// [`CheckpointCoordinator::prepare`] with the immediate parent execution
    /// id (sub-execution isolation): the actor id is resolved hierarchically
    /// when the parent is known.
    pub async fn prepare_with_parent(
        &self,
        entity_id: &str,
        trigger: CheckpointTiming,
        parent_execution_id: Option<&str>,
    ) -> Result<CheckpointContext, CheckpointError> {
        let actor_id = self.file_checkpoint_manager.as_ref().map(|manager| {
            manager
                .resolve_actor(entity_id, parent_execution_id)
                .to_string()
        });
        Ok(CheckpointContext {
            entity_type: "workflow_execution".to_string(),
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

/// Sync metadata loader over a pre-built checkpoint metadata index, used by
/// the hierarchy BFS restore (which is a synchronous traversal).
struct MetadataIndexLoader {
    index: HashMap<String, CheckpointStorageMetadata>,
}

impl MetadataIndexLoader {
    fn new(index: HashMap<String, CheckpointStorageMetadata>) -> Self {
        Self { index }
    }
}

impl crate::restore::hierarchy::CheckpointLoader for MetadataIndexLoader {
    fn load_metadata(
        &self,
        id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        Ok(self.index.get(id).cloned())
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowExecutionEntity {
    pub execution_id: String,
    pub status: String,
    pub snapshot: WorkflowExecutionStateSnapshot,
    pub restore_summary: Option<RestoreSummary>,
    pub hierarchy_validation: Option<HierarchyValidationResult>,
    pub join_inference: Option<JoinStateInference>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::SizeBudget;
    use crate::event::CheckpointEvent;
    use crate::metadata_builder::{CREATED_AT_FIELD, FORMAT_VERSION_FIELD};
    use crate::version::VersionManager;
    use std::sync::Arc;
    use wf_storage::backend::StorageBackend;
    use wf_types::checkpoint::CheckpointTiming;

    fn make_snapshot() -> WorkflowExecutionStateSnapshot {
        WorkflowExecutionStateSnapshot {
            execution_id: "exec-1".to_string(),
            status: "running".to_string(),
            current_node_id: Some("node-1".to_string()),
            node_results: None,
            variable_state: wf_types::checkpoint::CheckpointVariableState {
                variables: std::collections::HashMap::new(),
            },
            input: None,
            output: None,
            messages: None,
            fork_join_context: None,
            active_operations: None,

            node_execution_records: None,
            conversation_state: None,
            trigger_states: None,
            error_records: None,
            interruption_records: None,
            event_records: None,
            hierarchy: None,
            execution_config: None,
            fork_join_aggregation_state: None,
            hook_execution_context: None,
            message_base_checkpoint_id: None,
            message_total_count: None,
            truncated: None,
            truncation_stats: None,
        }
    }

    fn make_coordinator() -> WorkflowCheckpointCoordinator {
        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        WorkflowCheckpointCoordinator::new(sm)
    }

    #[tokio::test]
    async fn prepare_returns_context() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        assert_eq!(ctx.entity_type, "workflow_execution");
        assert_eq!(ctx.entity_id, "exec-1");
    }

    #[tokio::test]
    async fn build_creates_full_checkpoint_on_first_save() {
        let coord = make_coordinator();
        let ctx = CheckpointContext {
            entity_type: "workflow_execution".to_string(),
            entity_id: "exec-1".to_string(),
            trigger: None,
            actor_id: None,
            attempt: None,
            retry_count: None,
            error: None,
            fallback_used: None,
            metadata: None,
        };
        let checkpoint = coord.build(ctx, make_snapshot()).await.unwrap();
        assert_eq!(checkpoint.r#type, Some(CheckpointType::Full));
        assert!(checkpoint.snapshot.is_some());
        assert!(checkpoint.format_version.is_some());
    }

    #[tokio::test]
    async fn persist_saves_to_storage() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let loaded = coord.state_manager().load(&cp.id).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn restore_from_full_checkpoint() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        let id = cp.id.clone();
        coord.persist(&cp, "exec-1").await.unwrap();

        let entity = coord.restore(&id).await.unwrap();
        assert_eq!(entity.execution_id, "exec-1");
        assert_eq!(entity.status, "running");
    }

    #[tokio::test]
    async fn determine_type_respects_config() {
        let coord = make_coordinator();
        let config = DeltaStorageConfig {
            enabled: false,
            baseline_interval: 5,
            max_delta_chain_length: 10,
        };
        let tp = coord.determine_type("exec-1", &config).await.unwrap();
        assert_eq!(tp, CheckpointType::Full);

        let config_enabled = DeltaStorageConfig {
            enabled: true,
            baseline_interval: 5,
            max_delta_chain_length: 10,
        };
        let tp = coord
            .determine_type("exec-1", &config_enabled)
            .await
            .unwrap();
        assert_eq!(tp, CheckpointType::Full);

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let tp = coord
            .determine_type("exec-1", &config_enabled)
            .await
            .unwrap();
        assert_eq!(tp, CheckpointType::Delta);

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let tp = coord
            .determine_type("exec-1", &config_enabled)
            .await
            .unwrap();
        assert_eq!(tp, CheckpointType::Full);
    }

    #[tokio::test]
    async fn persist_emits_event() {
        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let bus = CheckpointEventBus::new();
        let coord = WorkflowCheckpointCoordinator::new(sm).with_event_bus(bus.clone());

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        assert_eq!(bus.receiver_count(), 0);
    }

    async fn build_and_persist(
        coord: &WorkflowCheckpointCoordinator,
        status: &str,
        node: &str,
    ) -> WorkflowCheckpoint {
        let mut snapshot = make_snapshot();
        snapshot.status = status.to_string();
        snapshot.current_node_id = Some(node.to_string());
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();
        cp
    }

    #[tokio::test]
    async fn delta_chain_restore_after_multiple_deltas() {
        let coord = make_coordinator();
        build_and_persist(&coord, "running", "node-1").await;
        build_and_persist(&coord, "running", "node-2").await;
        let cp3 = build_and_persist(&coord, "completed", "node-3").await;

        assert_eq!(cp3.r#type, Some(CheckpointType::Delta));
        assert!(cp3.base_checkpoint_id.is_some());

        let entity = coord.restore(&cp3.id).await.unwrap();
        assert_eq!(entity.status, "completed");
        assert_eq!(entity.snapshot.current_node_id, Some("node-3".to_string()));
    }

    #[tokio::test]
    async fn delta_chain_base_points_to_snapshot_checkpoint() {
        let coord = make_coordinator();
        let cp1 = build_and_persist(&coord, "running", "node-1").await;
        build_and_persist(&coord, "running", "node-2").await;
        let cp3 = build_and_persist(&coord, "completed", "node-3").await;

        assert_eq!(cp1.r#type, Some(CheckpointType::Full));
        assert_eq!(cp3.r#type, Some(CheckpointType::Delta));
        assert_eq!(cp3.base_checkpoint_id.as_deref(), Some(cp1.id.as_str()));
    }

    #[tokio::test]
    async fn baseline_interval_forces_periodic_full() {
        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let config = DeltaStorageConfig {
            enabled: true,
            baseline_interval: 2,
            max_delta_chain_length: 5,
        };
        let coord = WorkflowCheckpointCoordinator::new(sm).with_delta_config(config);

        let cp1 = build_and_persist(&coord, "running", "node-1").await;
        let cp2 = build_and_persist(&coord, "running", "node-2").await;
        let cp3 = build_and_persist(&coord, "running", "node-3").await;

        assert_eq!(cp1.r#type, Some(CheckpointType::Full));
        assert_eq!(cp2.r#type, Some(CheckpointType::Delta));
        assert_eq!(cp3.r#type, Some(CheckpointType::Full));
    }

    #[tokio::test]
    async fn restore_after_periodic_baseline() {
        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let config = DeltaStorageConfig {
            enabled: true,
            baseline_interval: 2,
            max_delta_chain_length: 5,
        };
        let coord = WorkflowCheckpointCoordinator::new(sm).with_delta_config(config);

        build_and_persist(&coord, "running", "node-1").await;
        build_and_persist(&coord, "running", "node-2").await;
        build_and_persist(&coord, "running", "node-3").await;
        let cp4 = build_and_persist(&coord, "completed", "node-4").await;

        assert_eq!(cp4.r#type, Some(CheckpointType::Delta));

        let entity = coord.restore(&cp4.id).await.unwrap();
        assert_eq!(entity.status, "completed");
        assert_eq!(entity.snapshot.current_node_id, Some("node-4".to_string()));
    }

    #[tokio::test]
    async fn fallback_to_full_when_chain_base_missing() {
        let coord = make_coordinator();
        let cp1 = build_and_persist(&coord, "running", "node-1").await;
        let cp2 = build_and_persist(&coord, "running", "node-2").await;
        assert_eq!(cp2.r#type, Some(CheckpointType::Delta));

        coord.state_manager().delete(&cp1.id).await.unwrap();

        let cp3 = build_and_persist(&coord, "running", "node-3").await;
        assert_eq!(cp3.r#type, Some(CheckpointType::Full));
        assert!(cp3.snapshot.is_some());

        let entity = coord.restore(&cp3.id).await.unwrap();
        assert_eq!(entity.snapshot.current_node_id, Some("node-3".to_string()));
    }

    fn make_policy(triggers: Vec<CheckpointTiming>) -> UnifiedCheckpointPolicy {
        UnifiedCheckpointPolicy {
            enabled: true,
            triggers,
            content: None,
            retention: None,
            error_handling: None,
        }
    }

    #[tokio::test]
    async fn strategy_skips_unconfigured_trigger() {
        let coord =
            make_coordinator().with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]));

        let skipped = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::BeforeExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(skipped.is_none());

        let created = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::AfterExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(created.is_some());
    }

    #[tokio::test]
    async fn strategy_disabled_never_checkpoints() {
        let coord = make_coordinator().with_strategy(&UnifiedCheckpointPolicy {
            enabled: false,
            triggers: vec![CheckpointTiming::AfterExecute],
            content: None,
            retention: None,
            error_handling: None,
        });

        let result = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::AfterExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn no_strategy_always_checkpoints() {
        let coord = make_coordinator();
        let result = coord
            .create_checkpoint_with_strategy(CheckpointTiming::Manual, "exec-1", make_snapshot())
            .await
            .unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn size_budget_truncates_messages_in_build() {
        use wf_types::message::{Message, MessageContentValue, MessageRole};

        let make_message = |id: &str| Message {
            id: id.to_string(),
            role: MessageRole::User,
            content: MessageContentValue::Text(format!("msg {}", id)),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };

        let mut snapshot = make_snapshot();
        snapshot.messages = Some(vec![
            make_message("m1"),
            make_message("m2"),
            make_message("m3"),
        ]);

        let budget = SizeBudget::new(10 * 1024 * 1024, 2);
        let coord = make_coordinator().with_size_budget(budget);

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        let restored = cp.snapshot.unwrap();
        assert_eq!(restored.messages.as_ref().map(|m| m.len()), Some(2));
        assert_eq!(
            restored.messages.unwrap()[0].id,
            "m2",
            "keeps the tail of the message history"
        );
    }

    #[tokio::test]
    async fn oversize_snapshot_degrades_delta_to_full() {
        let coord = make_coordinator();
        build_and_persist(&coord, "running", "node-1").await;

        let budget = SizeBudget::new(64, 100);
        let coord = make_coordinator().with_size_budget(budget);

        let mut snapshot = make_snapshot();
        snapshot.node_results = Some(HashMap::from([(
            "big".to_string(),
            serde_json::json!({"payload": "x".repeat(4096)}),
        )]));

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        assert_eq!(
            cp.r#type,
            Some(CheckpointType::Full),
            "oversize snapshot degrades to FULL"
        );
        assert!(cp.snapshot.is_some());
    }

    #[tokio::test]
    async fn restore_migrates_old_format_version() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
        cp.format_version = Some("1.0.0".to_string());
        coord.persist(&cp, "exec-1").await.unwrap();

        let entity = coord.restore(&cp.id).await.unwrap();
        assert_eq!(entity.execution_id, "exec-1");
    }

    #[tokio::test]
    async fn restore_rejects_incompatible_version() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
        cp.format_version = Some("0.5.0".to_string());
        coord.persist(&cp, "exec-1").await.unwrap();

        let err = coord.restore(&cp.id).await.unwrap_err();
        assert!(matches!(err, CheckpointError::VersionIncompatible { .. }));
    }

    #[tokio::test]
    async fn restore_future_version_rejected() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
        cp.format_version = Some("9.0.0".to_string());
        coord.persist(&cp, "exec-1").await.unwrap();

        assert!(matches!(
            coord.restore(&cp.id).await.unwrap_err(),
            CheckpointError::VersionIncompatible { .. }
        ));
    }

    fn make_message(id: &str, text: &str) -> Message {
        use wf_types::message::MessageContentValue;
        use wf_types::message::MessageRole;
        Message {
            id: id.to_string(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn restore_rebuilds_message_chain() {
        let coord = make_coordinator();

        // FULL checkpoint with a message base + truncated messages link.
        let mut base = make_snapshot();
        base.messages = Some(vec![make_message("m1", "hello")]);
        base.message_total_count = Some(1);
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp1 = coord.build(ctx, base).await.unwrap();
        coord.persist(&cp1, "exec-1").await.unwrap();

        // DELTA checkpoint whose snapshot has no messages but links back to
        // the base via the message chain.
        let mut delta_state = make_snapshot();
        delta_state.status = "completed".to_string();
        delta_state.messages = None;
        delta_state.message_base_checkpoint_id = Some(cp1.id.clone());
        delta_state.message_total_count = Some(2);
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp2 = coord.build(ctx, delta_state).await.unwrap();
        coord.persist(&cp2, "exec-1").await.unwrap();

        let entity = coord.restore(&cp2.id).await.unwrap();
        assert_eq!(entity.status, "completed");
        assert_eq!(
            entity.snapshot.messages.as_ref().map(|m| m.len()),
            Some(1),
            "rebuilds messages from the base checkpoint"
        );
        assert_eq!(entity.snapshot.messages.unwrap()[0].id, "m1");
    }

    #[tokio::test]
    async fn restore_rebuilds_message_chain_with_modified_and_deleted() {
        let coord = make_coordinator();

        // FULL checkpoint holding the original messages.
        let mut base = make_snapshot();
        base.messages = Some(vec![
            make_message("m1", "original-1"),
            make_message("m2", "original-2"),
            make_message("m3", "original-3"),
        ]);
        base.message_total_count = Some(3);
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp1 = coord.build(ctx, base).await.unwrap();
        coord.persist(&cp1, "exec-1").await.unwrap();
        assert_eq!(cp1.r#type, Some(CheckpointType::Full));

        // DELTA checkpoint that modifies m2 and deletes m1 (the diff against
        // the base produces the modified/deleted ops).
        let mut edited_state = make_snapshot();
        edited_state.status = "running".to_string();
        edited_state.messages = Some(vec![
            make_message("m2", "edited-2"),
            make_message("m3", "original-3"),
        ]);
        edited_state.message_total_count = Some(2);
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp2 = coord.build(ctx, edited_state).await.unwrap();
        coord.persist(&cp2, "exec-1").await.unwrap();
        assert_eq!(cp2.r#type, Some(CheckpointType::Delta));
        assert_eq!(
            cp2.delta
                .as_ref()
                .and_then(|d| d.deleted_message_indices.as_ref()),
            Some(&vec![0]),
            "delta records the deletion of m1"
        );

        // Terminal checkpoint whose snapshot omits messages but links back to
        // the base through the message chain.
        let mut delta_state = make_snapshot();
        delta_state.status = "completed".to_string();
        delta_state.messages = None;
        delta_state.message_base_checkpoint_id = Some(cp1.id.clone());
        delta_state.message_total_count = Some(2);
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp3 = coord.build(ctx, delta_state).await.unwrap();
        coord.persist(&cp3, "exec-1").await.unwrap();

        let entity = coord.restore(&cp3.id).await.unwrap();
        let messages = entity.snapshot.messages.expect("chain rebuilt");
        assert_eq!(
            messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["m2", "m3"],
            "deleted m1 replayed from the intermediate delta"
        );
        assert_eq!(
            messages[0].content,
            wf_types::message::MessageContentValue::Text("edited-2".to_string()),
            "modified message replaces in place"
        );
    }

    #[tokio::test]
    async fn restore_child_hierarchy_summary() {
        use wf_types::execution::{ChildExecutionReference, ExecutionHierarchy, ExecutionType};

        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let coord = WorkflowCheckpointCoordinator::new(sm);

        // Parent checkpoint.
        let mut snapshot = make_snapshot();
        snapshot.hierarchy = Some(ExecutionHierarchy {
            workflow_id: "wf-1".to_string(),
            execution_id: "exec-1".to_string(),
            parent_execution_id: None,
            depth: 0,
            root_execution_id: None,
            children: Some(vec![ChildExecutionReference {
                child_type: ExecutionType::Workflow,
                child_id: "child-exec-1".to_string(),
                created_at: 0,
                fork_path_id: None,
            }]),
        });
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        // Child checkpoint stored under the child execution id.
        let mut child_snapshot = make_snapshot();
        child_snapshot.execution_id = "child-exec-1".to_string();
        child_snapshot.status = "completed".to_string();
        let ctx = coord
            .prepare("child-exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let child_cp = coord.build(ctx, child_snapshot).await.unwrap();
        coord.persist(&child_cp, "child-exec-1").await.unwrap();

        let entity = coord.restore(&cp.id).await.unwrap();
        let summary = entity.restore_summary.unwrap();
        assert_eq!(summary.total, 1, "child checkpoint BFS-restored");
        assert_eq!(summary.success, 1);
    }

    #[tokio::test]
    async fn version_manager_is_exposed() {
        let coord = make_coordinator();
        assert_eq!(coord.version_manager().current_version(), "1.1.0");
        let vm = VersionManager::new();
        let coord = make_coordinator().with_version_manager(vm);
        assert_eq!(coord.version_manager().current_version(), "1.1.0");
    }

    #[tokio::test]
    async fn restore_registers_entity_and_validates_hierarchy() {
        use crate::restore::integrity::InMemoryExecutionRegistry;
        use std::sync::Arc;
        use wf_types::execution::{ChildExecutionReference, ExecutionHierarchy, ExecutionType};

        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let registry = Arc::new(InMemoryExecutionRegistry::new());
        let coord =
            WorkflowCheckpointCoordinator::new(sm).with_execution_registry(registry.clone());

        let mut snapshot = make_snapshot();
        snapshot.hierarchy = Some(ExecutionHierarchy {
            workflow_id: "wf-1".to_string(),
            execution_id: "exec-1".to_string(),
            parent_execution_id: None,
            depth: 0,
            root_execution_id: None,
            children: Some(vec![ChildExecutionReference {
                child_type: ExecutionType::Workflow,
                child_id: "child-exec-1".to_string(),
                created_at: 0,
                fork_path_id: None,
            }]),
        });
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let entity = coord.restore(&cp.id).await.unwrap();
        assert!(registry.has("exec-1"), "restored entity registered");
        let validation = entity.hierarchy_validation.unwrap();
        assert!(
            !validation.valid,
            "orphaned child reference reported as issue"
        );
        assert_eq!(validation.issues.len(), 1);
    }

    #[tokio::test]
    async fn restore_infers_join_state_from_registry() {
        use crate::restore::integrity::InMemoryExecutionRegistry;
        use std::sync::Arc;
        use wf_types::execution::{ChildExecutionReference, ExecutionHierarchy, ExecutionType};

        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let registry = Arc::new(InMemoryExecutionRegistry::new());
        let coord =
            WorkflowCheckpointCoordinator::new(sm).with_execution_registry(registry.clone());

        // Snapshot captured at a JOIN node with aggregation state.
        let mut snapshot = make_snapshot();
        snapshot.current_node_id = Some("join-1".to_string());
        snapshot.fork_join_aggregation_state = Some(serde_json::json!({
            "forkNodeId": "fork-1",
            "joinNodeId": "join-1",
            "pathStatuses": {"path-1": "PENDING", "path-2": "PENDING"},
            "isAggregationComplete": false,
        }));
        snapshot.hierarchy = Some(ExecutionHierarchy {
            workflow_id: "wf-1".to_string(),
            execution_id: "exec-1".to_string(),
            parent_execution_id: None,
            depth: 0,
            root_execution_id: None,
            children: Some(vec![
                ChildExecutionReference {
                    child_type: ExecutionType::Workflow,
                    child_id: "child-1".to_string(),
                    created_at: 0,
                    fork_path_id: Some("path-1".to_string()),
                },
                ChildExecutionReference {
                    child_type: ExecutionType::Workflow,
                    child_id: "child-2".to_string(),
                    created_at: 0,
                    fork_path_id: Some("path-2".to_string()),
                },
            ]),
        });
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        // Simulate that child executions were restored (completed) before the
        // parent's restore step runs the inference.
        registry.register_fork_path("child-1", ExecutionStatus::Completed, "exec-1", "path-1");
        registry.register_fork_path("child-2", ExecutionStatus::Completed, "exec-1", "path-2");

        let entity = coord.restore(&cp.id).await.unwrap();
        let inference = entity.join_inference.expect("join inference ran");
        assert_eq!(inference.completed_paths.len(), 2);
        assert!(inference.is_complete());
    }

    #[tokio::test]
    async fn restore_restores_file_checkpoint() {
        use crate::file::{FileCheckpointManager, FileContentEntry};
        use layertwine::storage::repository::CheckpointPersist;

        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);

        let file_storage =
            Arc::new(layertwine::storage::sqlite::SqliteStorage::new_full_in_memory().unwrap());
        let file_manager = FileCheckpointManager::with_sqlite(file_storage.clone());
        let file_manager2 = FileCheckpointManager::with_sqlite(file_storage.clone());
        file_manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"hello".to_vec())],
            )
            .unwrap();

        let coord =
            WorkflowCheckpointCoordinator::new(sm).with_file_checkpoint_manager(file_manager2);

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let entity = coord.restore(&cp.id).await.unwrap();
        assert_eq!(entity.execution_id, "exec-1");
        assert_eq!(
            file_storage.list_checkpoints().unwrap().len(),
            1,
            "layertwine checkpoint stored"
        );
    }

    #[tokio::test]
    async fn async_persistence_defers_file_snapshot_until_wait() {
        use crate::file::{FileCheckpointManager, FileContentEntry};
        use layertwine::storage::repository::CheckpointPersist;

        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let file_storage =
            Arc::new(layertwine::storage::sqlite::SqliteStorage::new_full_in_memory().unwrap());
        let file_manager = FileCheckpointManager::with_sqlite(file_storage.clone());
        file_manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"hello".to_vec())],
            )
            .unwrap();

        let coord = WorkflowCheckpointCoordinator::new(sm)
            .with_async_persistence(true)
            .with_file_checkpoint_manager(file_manager);

        let id = coord
            .create_checkpoint(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
            .await
            .unwrap();
        assert_eq!(coord.pending_persistence_count().await, 1);
        assert_eq!(
            file_storage.list_checkpoints().unwrap().len(),
            1,
            "deferred file snapshot not yet written"
        );

        coord.wait_for_persistence().await;
        assert_eq!(coord.pending_persistence_count().await, 0);
        assert_eq!(
            file_storage.list_checkpoints().unwrap().len(),
            2,
            "deferred file snapshot written after wait"
        );

        let loaded = coord.state_manager().load(&id).await.unwrap();
        assert!(
            loaded.is_some(),
            "checkpoint itself persisted synchronously"
        );
    }

    #[tokio::test]
    async fn async_persistence_enabled_via_policy_content_config() {
        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let policy = UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![CheckpointTiming::AfterExecute],
            content: Some(wf_types::checkpoint::CheckpointContentConfig {
                include_state: Some(true),
                include_history: Some(true),
                include_statistics: Some(false),
                metadata: None,
                asynchronous: Some(true),
            }),
            retention: None,
            error_handling: None,
        };
        let coord = WorkflowCheckpointCoordinator::new(sm).with_strategy(&policy);
        assert!(coord.async_persistence_enabled());

        coord
            .create_checkpoint(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
            .await
            .unwrap();
        assert_eq!(coord.pending_persistence_count().await, 1);
        coord.wait_for_persistence().await;
        assert_eq!(coord.pending_persistence_count().await, 0);
    }

    #[tokio::test]
    async fn build_writes_metadata_with_trigger_and_chain_position() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::OnError)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();

        let metadata = cp.metadata.unwrap();
        assert_eq!(
            metadata.get("description").and_then(|v| v.as_str()),
            Some("Error checkpoint"),
            "trigger-based description"
        );
        assert_eq!(
            metadata.get("tags"),
            Some(&serde_json::json!(["trigger:ON_ERROR"]))
        );
        let custom = metadata.get("customFields").unwrap().as_object().unwrap();
        assert_eq!(
            custom.get(FORMAT_VERSION_FIELD).and_then(|v| v.as_str()),
            Some("1.1.0")
        );
        assert!(custom.get(CREATED_AT_FIELD).is_some());
        assert_eq!(
            custom.get(CHAIN_POSITION_FIELD),
            Some(&serde_json::json!(0))
        );
    }

    #[tokio::test]
    async fn caller_custom_fields_are_merged_into_metadata() {
        let coord = make_coordinator();
        let ctx = CheckpointContext {
            entity_type: "workflow_execution".to_string(),
            entity_id: "exec-1".to_string(),
            trigger: None,
            actor_id: None,
            attempt: None,
            retry_count: None,
            error: None,
            fallback_used: None,
            metadata: Some(std::collections::HashMap::from([(
                "nodeId".to_string(),
                serde_json::json!("node-7"),
            )])),
        };
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        let metadata = cp.metadata.unwrap();
        let custom = metadata.get("customFields").unwrap().as_object().unwrap();
        assert_eq!(custom.get("nodeId"), Some(&serde_json::json!("node-7")));
    }
    #[tokio::test]
    async fn create_checkpoint_aggregate_persists_and_returns_id() {
        let coord = make_coordinator();
        let id = coord
            .create_checkpoint(CheckpointTiming::AfterExecute, "exec-1", make_snapshot())
            .await
            .unwrap();
        assert!(!id.is_empty());
        assert!(coord.state_manager().load(&id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn create_checkpoint_with_strategy_persists_saved_id() {
        let coord =
            make_coordinator().with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]));

        let created = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::AfterExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(coord
            .state_manager()
            .load(&created)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn restore_rejects_invalid_delta_checkpoint() {
        let coord = make_coordinator();
        let cp1 = build_and_persist(&coord, "running", "node-1").await;

        let mut invalid = coord
            .build(
                coord
                    .prepare("exec-1", CheckpointTiming::AfterExecute)
                    .await
                    .unwrap(),
                make_snapshot(),
            )
            .await
            .unwrap();
        invalid.r#type = Some(CheckpointType::Delta);
        invalid.base_checkpoint_id = Some(cp1.id.clone());
        invalid.previous_checkpoint_id = None;
        invalid.snapshot = None;
        invalid.delta = None;
        coord.persist(&invalid, "exec-1").await.unwrap();

        let err = coord.restore(&invalid.id).await.unwrap_err();
        assert!(
            matches!(err, CheckpointError::Validation { .. }),
            "missing previous_checkpoint_id rejected before restore"
        );
    }

    #[tokio::test]
    async fn failed_event_factory_carries_correlation_fields() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();

        bus.publish(CheckpointEventBus::failed_with(
            Some("cp-1".to_string()),
            "create",
            "persist failed: boom",
            Some("exec-1".to_string()),
        ));

        let event = rx.try_recv().unwrap();
        match event {
            CheckpointEvent::Failed { data, .. } => {
                assert_eq!(data.checkpoint_id.as_deref(), Some("cp-1"));
                assert_eq!(data.operation.as_deref(), Some("create"));
                assert_eq!(data.error.as_deref(), Some("persist failed: boom"));
                assert_eq!(data.execution_id.as_deref(), Some("exec-1"));
            }
            other => panic!("expected Failed event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn cadence_gates_strategy_created_checkpoints() {
        let coord = make_coordinator()
            .with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]))
            .with_cadence(CheckpointTiming::AfterExecute, 2);

        // Attempts increment on every call: 1 skipped, 2 fires, 3 skipped.
        let skipped = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::AfterExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(skipped.is_none(), "attempt 1 skipped");

        let created = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::AfterExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(created.is_some(), "attempt 2 fires");

        let skipped = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::AfterExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(skipped.is_none(), "attempt 3 skipped");
    }
}

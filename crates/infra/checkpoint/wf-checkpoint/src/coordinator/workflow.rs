use crate::coordinator::base::{
    decide_checkpoint_type_by_count, next_chain_position, publish_persist_failed,
    publish_persisted, restored_status, status_or_warn_running,
};
use crate::coordinator::CheckpointCoordinator;
use checkpoint_base::delta::CheckpointLoader;
use checkpoint_base::delta::DeltaRestorer;
use checkpoint_base::delta::DiffCalculator;
use checkpoint_base::delta::GenericDeltaRestorer;
use checkpoint_base::delta::WorkflowDiffCalculator;
use checkpoint_base::error::CheckpointError;
use checkpoint_base::metadata::builder::{
    build_checkpoint_metadata, fingerprint_entries, fingerprint_option, trigger_description,
    trigger_tag, CHAIN_POSITION_FIELD, WF_CURRENT_NODE_FIELD, WF_NODE_RESULTS_HASH_FIELD,
    WF_RECORD_COUNT_FIELD, WF_STATUS_FIELD, WF_TRIGGER_STATES_HASH_FIELD, WF_VARIABLES_HASH_FIELD,
};
use checkpoint_base::serializer::CheckpointSerializer;
use checkpoint_base::strategy::CheckpointStrategy;
use checkpoint_base::strategy::StandardStrategy;
use checkpoint_base::version_manager::VersionManager;
use checkpoint_base::version_manager::MIN_COMPATIBLE_VERSION;
use checkpoint_file::event::CheckpointEventBus;
use checkpoint_file::file::FileCheckpointManager;
use checkpoint_state::restore::fork_join::{ForkJoinStateInference, JoinStateInference};
use checkpoint_state::restore::hierarchy::{
    HierarchyRestorer, RestoreSummary, StorageChildResolver,
};
use checkpoint_state::restore::integrity::{
    ExecutionRegistry, HierarchyIntegrityService, HierarchyValidationResult,
};
use checkpoint_state::restore::registry::RestoreStrategyRegistry;
use checkpoint_state::state::CheckpointStateManager;
use checkpoint_state::state::WorkflowCheckpoint;
use checkpoint_state::state::WorkflowCheckpointStateManager;
use std::collections::{BTreeMap, HashMap, HashSet};
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
use wf_types::storage::CheckpointStorageMetadata;

/// Progress coordinates of a workflow checkpoint: execution status, resume
/// pointer, content hashes of node results and variables, the audit record
/// count and the trigger-state hash — read from metadata without loading
/// blobs. Equal coordinates mean no side effect landed since the recorded
/// checkpoint. Rows predating the coordinate fields read as `None` and never
/// compare equal to a fresh build, so dedup over old history is fail-open.
///
/// The resume pointer (`current_node_id`) is deliberately included: the row
/// after node A and the row before node B differ only in it, and their
/// restore paths differ (completed-node skip with successor re-derivation
/// versus direct continuation), so they are not duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowProgressCoords {
    pub status: Option<String>,
    pub current_node: Option<String>,
    pub node_results_hash: Option<String>,
    pub variables_hash: Option<String>,
    pub record_count: Option<u64>,
    pub trigger_states_hash: Option<String>,
}

impl WorkflowProgressCoords {
    /// Render coordinates as stored custom fields (`None` as JSON null,
    /// mirroring what `build` injects) for the shared gate comparison over
    /// [`checkpoint_base::metadata::builder::WF_PROGRESS_COORD_KEYS`].
    pub fn as_fields(&self) -> HashMap<String, serde_json::Value> {
        HashMap::from([
            (WF_STATUS_FIELD.to_string(), serde_json::json!(self.status)),
            (
                WF_CURRENT_NODE_FIELD.to_string(),
                serde_json::json!(self.current_node),
            ),
            (
                WF_NODE_RESULTS_HASH_FIELD.to_string(),
                serde_json::json!(self.node_results_hash),
            ),
            (
                WF_VARIABLES_HASH_FIELD.to_string(),
                serde_json::json!(self.variables_hash),
            ),
            (
                WF_RECORD_COUNT_FIELD.to_string(),
                serde_json::json!(self.record_count),
            ),
            (
                WF_TRIGGER_STATES_HASH_FIELD.to_string(),
                serde_json::json!(self.trigger_states_hash),
            ),
        ])
    }
}

/// Progress coordinates from stored checkpoint metadata.
pub fn workflow_progress_coords(meta: &CheckpointStorageMetadata) -> WorkflowProgressCoords {
    let get = |key: &str| {
        meta.custom_fields
            .as_ref()
            .and_then(|fields| fields.get(key))
    };
    WorkflowProgressCoords {
        status: get(WF_STATUS_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        current_node: get(WF_CURRENT_NODE_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        node_results_hash: get(WF_NODE_RESULTS_HASH_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        variables_hash: get(WF_VARIABLES_HASH_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        record_count: get(WF_RECORD_COUNT_FIELD).and_then(|v| v.as_u64()),
        trigger_states_hash: get(WF_TRIGGER_STATES_HASH_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

/// Hash one string-keyed value map with sorted keys so map iteration order
/// never affects the fingerprint. Same-key value changes alter the hash, so
/// counter-style variable overwrites still force a new row.
fn hash_value_map(map: &HashMap<String, serde_json::Value>) -> String {
    let entries: BTreeMap<String, Vec<u8>> = map
        .iter()
        .map(|(key, value)| (key.clone(), serde_json::to_vec(value).unwrap_or_default()))
        .collect();
    fingerprint_entries(&entries)
}

/// Progress coordinates of a not-yet-persisted snapshot. Mirrors the
/// coordinate fields `build` injects, so a live snapshot can be compared
/// against stored metadata before any blob is written. Computed from the
/// pre-policy snapshot: content filtering may strip blob domains, but the
/// coordinates describe the execution state, not the stored payload.
pub fn snapshot_workflow_coords(
    snapshot: &WorkflowExecutionStateSnapshot,
) -> WorkflowProgressCoords {
    let empty: HashMap<String, serde_json::Value> = HashMap::new();
    WorkflowProgressCoords {
        status: Some(snapshot.status.clone()),
        current_node: snapshot.current_node_id.clone(),
        node_results_hash: Some(hash_value_map(
            snapshot.node_results.as_ref().unwrap_or(&empty),
        )),
        variables_hash: Some(hash_value_map(&snapshot.variable_state.variables)),
        record_count: Some(
            snapshot
                .node_execution_records
                .as_ref()
                .map(Vec::len)
                .unwrap_or(0) as u64,
        ),
        trigger_states_hash: Some(fingerprint_option(&snapshot.trigger_states)),
    }
}

pub struct WorkflowCheckpointCoordinator {
    state_manager: WorkflowCheckpointStateManager,
    diff_calculator:
        Arc<dyn DiffCalculator<WorkflowExecutionStateSnapshot, WorkflowCheckpointDelta>>,
    event_bus: Option<CheckpointEventBus>,
    delta_config: DeltaStorageConfig,
    version_manager: VersionManager,
    strategy: Option<StandardStrategy>,
    error_handler: crate::error_handling::CheckpointErrorHandler,
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
            error_handler: crate::error_handling::CheckpointErrorHandler::default(),
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
            let filter = crate::common::content::ContentFilter::new();
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
        // Storage bytes may be gzip-compressed; migration handlers expect
        // plain encoded bytes, so normalize first.
        let raw = self
            .state_manager
            .load_checkpoint_data(checkpoint_id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            })?;
        let raw = CheckpointSerializer::decompressed(&raw)?;
        let migrated = self.version_manager.migrate_data(&raw, version).await?;
        CheckpointSerializer::auto_deserialize(&migrated)
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
        let gate = Arc::new(ConcurrencyGate::new(CHILD_RESTORE_CONCURRENCY));
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
                    let status = restored_status(&value);
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

    /// Synchronous best-effort file projection for the entity. Missing file
    /// history yields `Ok` so the state checkpoint never fails. Success and
    /// failure are logged with the state checkpoint id for correlation.
    async fn save_file_snapshot(
        &self,
        checkpoint_id: &str,
        entity_id: &str,
    ) -> Result<(), CheckpointError> {
        if let Some(manager) = &self.file_checkpoint_manager {
            match manager.create_latest_file_checkpoint(entity_id)? {
                Some(file_checkpoint) => {
                    tracing::debug!(
                        entity_id = %entity_id,
                        checkpoint_id = %checkpoint_id,
                        file_checkpoint_id = %file_checkpoint.id,
                        "file projection correlated with state checkpoint"
                    );
                }
                None => {
                    tracing::debug!(
                        entity_id = %entity_id,
                        checkpoint_id = %checkpoint_id,
                        "no file history for state checkpoint"
                    );
                }
            }
        }
        Ok(())
    }

    /// Defer post-persist side effects (file snapshot) to the background
    /// persistence queue (async mode). The queue is bounded and shared with
    /// the agent coordinator.
    async fn enqueue_persistence(&self, checkpoint_id: &str, entity_id: &str) {
        let checkpoint_id = checkpoint_id.to_string();
        let entity_id = entity_id.to_string();
        let file_manager = self.file_checkpoint_manager.clone();
        // File-snapshot creation is blocking file I/O; run it on the blocking
        // pool so a slow filesystem never pins a tokio worker.
        let handle = tokio::task::spawn_blocking(move || {
            if let Some(manager) = file_manager {
                match manager.create_latest_file_checkpoint(&entity_id) {
                    Ok(Some(file_checkpoint)) => {
                        tracing::debug!(
                            entity_id = %entity_id,
                            checkpoint_id = %checkpoint_id,
                            file_checkpoint_id = %file_checkpoint.id,
                            "deferred file projection correlated with state checkpoint"
                        );
                    }
                    Ok(None) => {
                        tracing::debug!(
                            entity_id = %entity_id,
                            checkpoint_id = %checkpoint_id,
                            "deferred file projection found no history"
                        );
                    }
                    Err(err) => {
                        tracing::warn!(
                            entity_id = %entity_id,
                            checkpoint_id = %checkpoint_id,
                            error = %err,
                            "deferred file checkpoint creation failed (best-effort)"
                        );
                    }
                }
            }
        });
        crate::coordinator::base::push_persistence_handle(&self.persistence_queue, handle).await;
    }

    /// Drain the persistence queue and wait for all deferred operations.
    async fn wait_for_persistence(&self) {
        crate::coordinator::base::drain_persistence_handles(&self.persistence_queue).await;
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
        self.apply_content_policy(&mut state);

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
        let mut custom_fields = ctx.metadata.clone().unwrap_or_default();
        custom_fields.insert(
            CHAIN_POSITION_FIELD.to_string(),
            serde_json::json!(chain_position),
        );
        custom_fields.insert(
            WF_STATUS_FIELD.to_string(),
            serde_json::json!(coords.status),
        );
        custom_fields.insert(
            WF_CURRENT_NODE_FIELD.to_string(),
            serde_json::json!(coords.current_node),
        );
        custom_fields.insert(
            WF_NODE_RESULTS_HASH_FIELD.to_string(),
            serde_json::json!(coords.node_results_hash),
        );
        custom_fields.insert(
            WF_VARIABLES_HASH_FIELD.to_string(),
            serde_json::json!(coords.variables_hash),
        );
        custom_fields.insert(
            WF_RECORD_COUNT_FIELD.to_string(),
            serde_json::json!(coords.record_count),
        );
        custom_fields.insert(
            WF_TRIGGER_STATES_HASH_FIELD.to_string(),
            serde_json::json!(coords.trigger_states_hash),
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
            publish_persist_failed(
                self.event_bus.as_ref(),
                Some(checkpoint.id.clone()),
                entity_id,
                &err,
            );
            // Route through the checkpoint error handler: the default
            // handler warns and swallows the failure so the execution
            // continues without a checkpoint.
            let context = self
                .error_handler
                .context("create", Some(checkpoint.id.clone()), None);
            let outcome = self.error_handler.decide(&context, &err);
            if outcome.should_rethrow {
                return Err(err);
            }
            return Ok(());
        }

        let description = checkpoint
            .metadata
            .as_ref()
            .and_then(|m| m.get("description"))
            .and_then(|v| v.as_str());
        publish_persisted(
            self.event_bus.as_ref(),
            &checkpoint.id,
            entity_id,
            description,
        );

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
                status_or_warn_running(&entity.status),
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
        // aggregate COUNT query instead of materializing the full
        // history listing.
        let count = self.state_manager.count_by_entity(entity_id).await?;
        Ok(decide_checkpoint_type_by_count(count, config))
    }

    fn default_strategy(&self) -> Option<&dyn CheckpointStrategy> {
        self.strategy.as_ref().map(|s| s as &dyn CheckpointStrategy)
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
        if let Some(manager) = &self.file_checkpoint_manager {
            manager
                .ensure_child_branch(entity_id, parent_execution_id)
                .await?;
        }
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

    /// Merge a caller-supplied description into an existing checkpoint row
    /// without allocating a new row. Shared implementation lives in
    /// [`crate::coordinator::base::merge_description_back`]; the contract
    /// (trigger label untouched, timestamp preserved, missing target falls
    /// back to current latest) is identical for both coordinators.
    pub async fn merge_description_back(
        &self,
        checkpoint_id: &str,
        entity_id: &str,
        description: &str,
    ) -> Result<CheckpointStorageMetadata, CheckpointError> {
        crate::coordinator::base::merge_description_back(
            &self.state_manager,
            checkpoint_id,
            "workflow_execution",
            entity_id,
            description,
        )
        .await
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
    use checkpoint_base::metadata::builder::{CREATED_AT_FIELD, FORMAT_VERSION_FIELD};
    use checkpoint_base::version_manager::VersionManager;
    use checkpoint_file::event::CheckpointEvent;
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
            message_contexts: None,
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
            error_suspend: None,
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
    async fn progress_coords_survive_build_persist_round_trip() {
        let coord = make_coordinator();
        let snapshot = make_snapshot();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot.clone()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let latest = coord
            .state_manager()
            .get_latest("exec-1")
            .await
            .unwrap()
            .expect("persisted checkpoint listed");
        assert_eq!(
            workflow_progress_coords(&latest),
            snapshot_workflow_coords(&snapshot)
        );
    }

    #[tokio::test]
    async fn merge_description_back_rewrites_user_text_only() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let merged = coord
            .merge_description_back(&cp.id, "exec-1", "second")
            .await
            .unwrap();
        assert_eq!(merged.id, cp.id);
        let description = merged
            .custom_fields
            .as_ref()
            .and_then(|fields| fields.get("description"))
            .and_then(|v| v.as_str());
        assert_eq!(description, Some("second"));
    }

    #[tokio::test]
    async fn merge_missing_target_returns_current_latest() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let first = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&first, "exec-1").await.unwrap();
        let ctx = coord
            .prepare("exec-1", CheckpointTiming::BeforeExecute)
            .await
            .unwrap();
        let second = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&second, "exec-1").await.unwrap();

        coord.state_manager().delete(&first.id).await.unwrap();
        let merged = coord
            .merge_description_back(&first.id, "exec-1", "late note")
            .await
            .unwrap();
        assert_eq!(merged.id, second.id);
    }

    #[tokio::test]
    async fn coords_match_full_snapshot_under_stripping_policy() {
        let storage = Arc::new(StorageBackend::new_memory());
        let sm = WorkflowCheckpointStateManager::new(storage);
        let policy = wf_types::checkpoint::UnifiedCheckpointPolicy {
            enabled: true,
            triggers: vec![],
            content: Some(wf_types::checkpoint::CheckpointContentConfig {
                include_state: Some(false),
                include_history: None,
                include_statistics: None,
                metadata: None,
                asynchronous: None,
            }),
            retention: None,
            error_handling: None,
        };
        let coord = WorkflowCheckpointCoordinator::new(sm).with_strategy(&policy);
        let mut snapshot = make_snapshot();
        snapshot.node_results = Some(HashMap::from([(
            "node-1".to_string(),
            serde_json::json!({"ok": true}),
        )]));
        snapshot
            .variable_state
            .variables
            .insert("counter".to_string(), serde_json::json!(1));

        let ctx = coord
            .prepare("exec-1", CheckpointTiming::AfterExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot.clone()).await.unwrap();
        // The blob payload is stripped, but the coordinates still describe
        // the pre-policy execution state.
        assert!(cp.snapshot.as_ref().unwrap().node_results.is_none());
        coord.persist(&cp, "exec-1").await.unwrap();

        let latest = coord
            .state_manager()
            .get_latest("exec-1")
            .await
            .unwrap()
            .expect("persisted checkpoint listed");
        assert_eq!(
            workflow_progress_coords(&latest),
            snapshot_workflow_coords(&snapshot)
        );
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
            ancestors: None,
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
        use checkpoint_state::restore::integrity::InMemoryExecutionRegistry;
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
            ancestors: None,
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
        use checkpoint_state::restore::integrity::InMemoryExecutionRegistry;
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
            ancestors: None,
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
        use checkpoint_file::file::{FileCheckpointManager, FileContentEntry};
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
        use checkpoint_file::file::{FileCheckpointManager, FileContentEntry};
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
    async fn strategy_gates_created_checkpoints_without_second_cadence_layer() {
        let coord =
            make_coordinator().with_strategy(&make_policy(vec![CheckpointTiming::AfterExecute]));

        let created = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::AfterExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(created.is_some(), "configured trigger fires");

        let skipped = coord
            .create_checkpoint_with_strategy(
                CheckpointTiming::BeforeExecute,
                "exec-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(skipped.is_none(), "unconfigured trigger skipped");
    }
}

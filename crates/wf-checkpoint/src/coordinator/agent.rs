use crate::content::SizeBudget;
use crate::coordinator::CheckpointCoordinator;
use crate::delta::AgentDiffCalculator;
use crate::delta::CheckpointLoader;
use crate::delta::DeltaRestorer;
use crate::delta::DiffCalculator;
use crate::delta::GenericDeltaRestorer;
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::file::FileCheckpointManager;
use crate::restore::hierarchy::{HierarchyRestorer, RestoreSummary, StorageChildResolver};
use crate::restore::integrity::{
    ExecutionRegistry, HierarchyIntegrityService, HierarchyValidationResult,
};
use crate::restore::registry::RestoreStrategyRegistry;
use crate::serializer::CheckpointSerializer;
use crate::state::AgentCheckpoint;
use crate::state::AgentCheckpointStateManager;
use crate::state::CheckpointStateManager;
use crate::strategy::CheckpointStrategy;
use crate::strategy::StandardStrategy;
use crate::version::VersionManager;
use crate::version::MIN_COMPATIBLE_VERSION;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use wf_types::checkpoint::agent::AgentCheckpointDelta;
use wf_types::checkpoint::agent::AgentStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::CheckpointContext;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::checkpoint::CheckpointType;
use wf_types::checkpoint::DeltaStorageConfig;
use wf_types::checkpoint::UnifiedCheckpointPolicy;
use wf_types::execution::ExecutionStatus;
use wf_types::storage::CheckpointStorageMetadata;

pub struct AgentCheckpointCoordinator {
    state_manager: AgentCheckpointStateManager,
    diff_calculator: Arc<dyn DiffCalculator<AgentStateSnapshot, AgentCheckpointDelta>>,
    event_bus: Option<CheckpointEventBus>,
    delta_config: DeltaStorageConfig,
    version_manager: VersionManager,
    strategy: Option<StandardStrategy>,
    size_budget: Option<SizeBudget>,
    restore_registry: Option<RestoreStrategyRegistry>,
    execution_registry: Option<Arc<dyn ExecutionRegistry>>,
    file_checkpoint_manager: Option<FileCheckpointManager>,
}

impl AgentCheckpointCoordinator {
    pub fn new(state_manager: AgentCheckpointStateManager) -> Self {
        Self {
            state_manager,
            diff_calculator: Arc::new(AgentDiffCalculator::new()),
            event_bus: None,
            delta_config: DeltaStorageConfig::default(),
            version_manager: VersionManager::new(),
            strategy: None,
            size_budget: None,
            restore_registry: None,
            execution_registry: None,
            file_checkpoint_manager: None,
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
    /// A disabled policy yields a strategy that never checkpoints.
    pub fn with_strategy(mut self, policy: &UnifiedCheckpointPolicy) -> Self {
        self.strategy = Some(crate::strategy::create_checkpoint_strategy(policy));
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
    /// validation after restore.
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

    pub fn state_manager(&self) -> &AgentCheckpointStateManager {
        &self.state_manager
    }

    pub fn version_manager(&self) -> &VersionManager {
        &self.version_manager
    }

    fn apply_content_policy(&self, state: &mut AgentStateSnapshot) {
        if let Some(strategy) = &self.strategy {
            let filter = crate::content::ContentFilter::new();
            let config = strategy.content_config();
            if !filter.should_include_state(config) {
                state.conversation_snapshot = None;
                state.messages = None;
                state.tool_call_history = None;
                state.variable_snapshots = None;
                state.error = None;
                state.error_records = None;
                state.interruption_records = None;
                state.event_records = None;
                state.iteration_history = None;
                state.current_iteration_record = None;
                state.stream_message = None;
                state.pending_tool_call_ids = None;
                state.trigger_state = None;
            }
            if !filter.should_include_history(config) {
                state.conversation_snapshot = None;
                state.messages = None;
                state.iteration_history = None;
            }
        }
        if let Some(budget) = &self.size_budget {
            if let Some(messages) = &state.messages {
                state.messages = budget.truncate_messages(Some(messages.clone()));
            }
            if let Some(snapshot) = &state.conversation_snapshot {
                state.conversation_snapshot = budget.truncate_messages(Some(snapshot.clone()));
            }
        }
    }

    /// When a size budget is configured and the snapshot still exceeds it
    /// after truncation, degrade the storage type to FULL.
    fn snapshot_over_budget(&self, state: &AgentStateSnapshot) -> bool {
        match &self.size_budget {
            Some(budget) => serde_json::to_vec(state)
                .map(|bytes| !budget.is_within_budget(bytes.len()))
                .unwrap_or(false),
            None => false,
        }
    }

    /// Load the checkpoint blob and bring it to the current format version.
    async fn load_migrated(&self, checkpoint_id: &str) -> Result<AgentCheckpoint, CheckpointError> {
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

        let raw = self
            .state_manager
            .load_checkpoint_data(checkpoint_id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            })?;
        let migrated = self.version_manager.migrate_data(&raw, version).await?;
        Ok(CheckpointSerializer::auto_deserialize(&migrated)?)
    }

    /// Post-restore phase: restore child executions through hierarchy metadata
    /// with BFS `HierarchyRestorer` plus the registered restore strategies.
    /// Restored children are registered into the execution registry for
    /// hierarchy integrity validation.
    async fn restore_child_hierarchy(
        &self,
        checkpoint_id: &str,
        parent_entity_id: &str,
        hierarchy: &wf_types::execution::ExecutionHierarchy,
        registry: Option<&Arc<dyn ExecutionRegistry>>,
    ) -> Result<RestoreSummary, CheckpointError> {
        let children = hierarchy.children.clone().unwrap_or_default();
        if children.is_empty() {
            return Ok(RestoreSummary {
                total: 0,
                success: 0,
                failed: 0,
            });
        }

        let resolver = StorageChildResolver::new();
        let mut index: HashMap<String, CheckpointStorageMetadata> = HashMap::new();
        let mut restored = 0u32;

        for child in &children {
            if let Some(meta) = self.state_manager.get_latest(&child.child_id).await? {
                index.insert(meta.id.clone(), meta.clone());
                resolver.register_relationship(checkpoint_id, &meta.id);

                if let Some(reg) = &self.restore_registry {
                    let entity_type = match child.child_type {
                        wf_types::execution::ExecutionType::Workflow => "workflow_execution",
                        wf_types::execution::ExecutionType::AgentLoop => "agent_loop",
                    };
                    if let Some(data) = self.state_manager.load_checkpoint_data(&meta.id).await? {
                        let restore_result = reg.restore(entity_type, &meta.id, &data).await;
                        if restore_result.is_ok() {
                            restored += 1;
                        }
                        if let Some(exec_registry) = registry {
                            let status = restore_result
                                .ok()
                                .and_then(|value| parse_execution_status(&value));
                            register_child(
                                exec_registry.as_ref(),
                                &child.child_id,
                                status,
                                parent_entity_id,
                                child.fork_path_id.as_deref(),
                            );
                        }
                    }
                }
            }
        }

        let loader = MetadataIndexLoader::new(index);
        let restorer = HierarchyRestorer::new(Arc::new(resolver));
        let results = restorer.restore_children_bfs(checkpoint_id, &loader, 8, None)?;

        let mut summary = HierarchyRestorer::summarize_results(&results);
        summary.success += restored as usize;
        Ok(summary)
    }
}

fn parse_execution_status(value: &serde_json::Value) -> Option<ExecutionStatus> {
    let status = value.get("status")?.as_str()?;
    Some(match status {
        "completed" => ExecutionStatus::Completed,
        "failed" => ExecutionStatus::Failed,
        "cancelled" => ExecutionStatus::Cancelled,
        "paused" => ExecutionStatus::Paused,
        "stopped" => ExecutionStatus::Stopped,
        "created" => ExecutionStatus::Created,
        _ => ExecutionStatus::Running,
    })
}

fn register_child(
    registry: &dyn ExecutionRegistry,
    child_id: &str,
    status: Option<ExecutionStatus>,
    parent: &str,
    fork_path_id: Option<&str>,
) {
    let status = status.unwrap_or(ExecutionStatus::Running);
    match fork_path_id {
        Some(path) => registry.register_fork_path(child_id, status, parent, path),
        None => registry.register_with_parent(child_id, status, Some(parent)),
    }
}

impl CheckpointCoordinator for AgentCheckpointCoordinator {
    type Checkpoint = AgentCheckpoint;
    type Entity = AgentLoopEntity;
    type State = AgentStateSnapshot;

    async fn prepare(
        &self,
        entity_id: &str,
        _trigger: CheckpointTrigger,
    ) -> Result<CheckpointContext, CheckpointError> {
        Ok(CheckpointContext {
            entity_type: "agent_loop".to_string(),
            entity_id: entity_id.to_string(),
            attempt: None,
            retry_count: None,
            error: None,
            fallback_used: None,
            metadata: None,
        })
    }

    async fn build(
        &self,
        ctx: CheckpointContext,
        mut state: Self::State,
    ) -> Result<Self::Checkpoint, CheckpointError> {
        // Content policy (ContentFilter + SizeBudget) applied before any
        // storage type decision is made.
        self.apply_content_policy(&mut state);

        let mut checkpoint_type = self
            .determine_type(&ctx.entity_id, &self.delta_config)
            .await?;
        if checkpoint_type == CheckpointType::Delta && self.snapshot_over_budget(&state) {
            checkpoint_type = CheckpointType::Full;
        }

        let previous = self.state_manager.get_latest(&ctx.entity_id).await?;

        match checkpoint_type {
            CheckpointType::Full => Ok(BaseCheckpointCore {
                id: wf_common::generate_id(),
                r#type: Some(CheckpointType::Full),
                base_checkpoint_id: None,
                previous_checkpoint_id: previous.map(|p| p.id),
                delta: None,
                snapshot: Some(state),
                timestamp: chrono::Utc::now().timestamp_millis(),
                metadata: None,
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
                            timestamp: chrono::Utc::now().timestamp_millis(),
                            metadata: None,
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
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        metadata: None,
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
        self.state_manager
            .save(checkpoint, "agent_loop", entity_id)
            .await?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::created(checkpoint.id.clone()));
        }

        Ok(())
    }

    async fn restore(&self, checkpoint_id: &str) -> Result<Self::Entity, CheckpointError> {
        let checkpoint = self.load_migrated(checkpoint_id).await?;

        let mut entity = match checkpoint.r#type {
            Some(CheckpointType::Full) => {
                let snapshot = checkpoint
                    .snapshot
                    .ok_or_else(|| CheckpointError::Corrupted {
                        id: checkpoint_id.to_string(),
                        reason: "full checkpoint missing snapshot".to_string(),
                    })?;

                Ok(AgentLoopEntity {
                    agent_loop_id: snapshot.agent_loop_id.clone(),
                    status: snapshot.status.clone(),
                    current_iteration: snapshot.current_iteration,
                    snapshot,
                    restore_summary: None,
                    hierarchy_validation: None,
                })
            }
            Some(CheckpointType::Delta) => {
                let restorer = GenericDeltaRestorer::new(self.diff_calculator.clone());
                let state = restorer
                    .restore_full_state(checkpoint_id, &self.state_manager)
                    .await?;

                Ok(AgentLoopEntity {
                    agent_loop_id: state.agent_loop_id.clone(),
                    status: state.status.clone(),
                    current_iteration: state.current_iteration,
                    snapshot: state,
                    restore_summary: None,
                    hierarchy_validation: None,
                })
            }
            None => Err(CheckpointError::Corrupted {
                id: checkpoint_id.to_string(),
                reason: "checkpoint has no type".to_string(),
            }),
        }?;

        // P2-5: register the restored entity into the execution registry,
        // restore child executions, then validate hierarchy integrity.
        let mut validation: Option<HierarchyValidationResult> = None;
        if let Some(registry) = &self.execution_registry {
            let hierarchy = entity.snapshot.hierarchy.clone();
            let parent = hierarchy
                .as_ref()
                .and_then(|h| h.parent_execution_id.clone());
            registry.register_with_parent(
                &entity.agent_loop_id,
                parse_status(&entity.status),
                parent.as_deref(),
            );

            // P2-4: post-restore phase — restore child executions from hierarchy.
            if let Some(h) = &hierarchy {
                if let Ok(summary) = self
                    .restore_child_hierarchy(
                        checkpoint_id,
                        &entity.agent_loop_id,
                        h,
                        Some(registry),
                    )
                    .await
                {
                    entity.restore_summary = Some(summary);
                }
                validation = Some(HierarchyIntegrityService::validate_integrity(
                    h,
                    registry.as_ref(),
                ));
            }
        } else if let Some(hierarchy) = entity.snapshot.hierarchy.clone() {
            // P2-4 fallback: restore children without a registry.
            if let Ok(summary) = self
                .restore_child_hierarchy(checkpoint_id, &entity.agent_loop_id, &hierarchy, None)
                .await
            {
                entity.restore_summary = Some(summary);
            }
        }
        entity.hierarchy_validation = validation;

        // P2-5: restore the latest file checkpoint for the entity (best-effort).
        if let Some(manager) = &self.file_checkpoint_manager {
            if let Err(err) = manager.restore_latest(&entity.agent_loop_id) {
                tracing::warn!(
                    "file checkpoint restore failed for entity {}: {}",
                    entity.agent_loop_id,
                    err
                );
            }
        }

        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::restored(
                checkpoint_id.to_string(),
                entity.agent_loop_id.clone(),
            ));
        }

        Ok(entity)
    }

    async fn delete(&self, checkpoint_id: &str) -> Result<bool, CheckpointError> {
        let deleted = self.state_manager.delete(checkpoint_id).await?;
        if deleted {
            if let Some(ref bus) = self.event_bus {
                bus.publish(CheckpointEventBus::deleted(checkpoint_id.to_string()));
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

        let count = self.state_manager.list_by_entity(entity_id).await?.len() as u32;
        let effective_interval = config.baseline_interval.min(config.max_delta_chain_length);

        if count == 0 || effective_interval == 0 || count % effective_interval == 0 {
            return Ok(CheckpointType::Full);
        }

        Ok(CheckpointType::Delta)
    }

    fn default_strategy(&self) -> Option<&dyn CheckpointStrategy> {
        self.strategy.as_ref().map(|s| s as &dyn CheckpointStrategy)
    }
}

impl AgentCheckpointCoordinator {
    async fn find_base(
        &self,
        previous: &Option<CheckpointStorageMetadata>,
    ) -> Result<(Option<String>, Option<AgentStateSnapshot>), CheckpointError> {
        let mut base_id: Option<String> = None;
        let mut base_snapshot: Option<AgentStateSnapshot> = None;
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
pub struct AgentLoopEntity {
    pub agent_loop_id: String,
    pub status: String,
    pub current_iteration: u32,
    pub snapshot: AgentStateSnapshot,
    pub restore_summary: Option<RestoreSummary>,
    pub hierarchy_validation: Option<HierarchyValidationResult>,
}

fn parse_status(status: &str) -> ExecutionStatus {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::SizeBudget;
    use wf_storage::backend::StorageBackend;
    use wf_types::checkpoint::CheckpointTrigger;

    fn make_snapshot() -> AgentStateSnapshot {
        AgentStateSnapshot {
            agent_loop_id: "loop-1".to_string(),
            status: "running".to_string(),
            current_iteration: 1,
            tool_call_count: 0,
            conversation_snapshot: None,
            tool_call_history: None,
            is_streaming: None,
            variable_snapshots: None,
            error: None,
            started_at: None,
            completed_at: None,
            error_records: None,
            interruption_records: None,
            event_records: None,
            iteration_history: None,
            current_iteration_record: None,
            stream_message: None,
            pending_tool_call_ids: None,
            trigger_state: None,
            hierarchy: None,
            messages: None,
        }
    }

    fn make_coordinator() -> AgentCheckpointCoordinator {
        let storage = Arc::new(StorageBackend::new_memory());
        let sm = AgentCheckpointStateManager::new(storage);
        AgentCheckpointCoordinator::new(sm)
    }

    #[tokio::test]
    async fn prepare_returns_context() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("loop-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        assert_eq!(ctx.entity_type, "agent_loop");
        assert_eq!(ctx.entity_id, "loop-1");
    }

    #[tokio::test]
    async fn build_creates_full_checkpoint_on_first_save() {
        let coord = make_coordinator();
        let ctx = CheckpointContext {
            entity_type: "agent_loop".to_string(),
            entity_id: "loop-1".to_string(),
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
            .prepare("loop-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "loop-1").await.unwrap();

        let loaded = coord.state_manager().load(&cp.id).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn restore_from_full_checkpoint() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("loop-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        let id = cp.id.clone();
        coord.persist(&cp, "loop-1").await.unwrap();

        let entity = coord.restore(&id).await.unwrap();
        assert_eq!(entity.agent_loop_id, "loop-1");
        assert_eq!(entity.status, "running");
    }

    #[tokio::test]
    async fn build_delta_after_full() {
        let coord = make_coordinator();

        let ctx1 = coord
            .prepare("loop-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let cp1 = coord.build(ctx1, make_snapshot()).await.unwrap();
        coord.persist(&cp1, "loop-1").await.unwrap();

        let mut snapshot2 = make_snapshot();
        snapshot2.current_iteration = 2;
        let ctx2 = coord
            .prepare("loop-1", CheckpointTrigger::AfterExecute)
            .await
            .unwrap();
        let cp2 = coord.build(ctx2, snapshot2).await.unwrap();
        assert_eq!(cp2.r#type, Some(CheckpointType::Delta));
        assert!(cp2.delta.is_some());
    }

    async fn build_and_persist(
        coord: &AgentCheckpointCoordinator,
        status: &str,
        iteration: u32,
    ) -> AgentCheckpoint {
        let mut snapshot = make_snapshot();
        snapshot.status = status.to_string();
        snapshot.current_iteration = iteration;
        let ctx = coord
            .prepare("loop-1", CheckpointTrigger::AfterExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        coord.persist(&cp, "loop-1").await.unwrap();
        cp
    }

    #[tokio::test]
    async fn delta_chain_restore_after_multiple_deltas() {
        let coord = make_coordinator();
        build_and_persist(&coord, "running", 1).await;
        build_and_persist(&coord, "running", 2).await;
        let cp3 = build_and_persist(&coord, "completed", 3).await;

        assert_eq!(cp3.r#type, Some(CheckpointType::Delta));

        let entity = coord.restore(&cp3.id).await.unwrap();
        assert_eq!(entity.status, "completed");
        assert_eq!(entity.current_iteration, 3);
    }

    #[tokio::test]
    async fn delta_chain_base_points_to_snapshot_checkpoint() {
        let coord = make_coordinator();
        let cp1 = build_and_persist(&coord, "running", 1).await;
        build_and_persist(&coord, "running", 2).await;
        let cp3 = build_and_persist(&coord, "completed", 3).await;

        assert_eq!(cp1.r#type, Some(CheckpointType::Full));
        assert_eq!(cp3.r#type, Some(CheckpointType::Delta));
        assert_eq!(cp3.base_checkpoint_id.as_deref(), Some(cp1.id.as_str()));
    }

    #[tokio::test]
    async fn fallback_to_full_when_chain_base_missing() {
        let coord = make_coordinator();
        let cp1 = build_and_persist(&coord, "running", 1).await;
        let cp2 = build_and_persist(&coord, "running", 2).await;
        assert_eq!(cp2.r#type, Some(CheckpointType::Delta));

        coord.state_manager().delete(&cp1.id).await.unwrap();

        let cp3 = build_and_persist(&coord, "running", 3).await;
        assert_eq!(cp3.r#type, Some(CheckpointType::Full));
        assert!(cp3.snapshot.is_some());

        let entity = coord.restore(&cp3.id).await.unwrap();
        assert_eq!(entity.current_iteration, 3);
    }

    #[tokio::test]
    async fn baseline_interval_forces_periodic_full() {
        let storage = Arc::new(StorageBackend::new_memory());
        let sm = AgentCheckpointStateManager::new(storage);
        let config = DeltaStorageConfig {
            enabled: true,
            baseline_interval: 2,
            max_delta_chain_length: 5,
        };
        let coord = AgentCheckpointCoordinator::new(sm).with_delta_config(config);

        let cp1 = build_and_persist(&coord, "running", 1).await;
        let cp2 = build_and_persist(&coord, "running", 2).await;
        let cp3 = build_and_persist(&coord, "running", 3).await;

        assert_eq!(cp1.r#type, Some(CheckpointType::Full));
        assert_eq!(cp2.r#type, Some(CheckpointType::Delta));
        assert_eq!(cp3.r#type, Some(CheckpointType::Full));
    }

    fn make_policy(triggers: Vec<CheckpointTrigger>) -> UnifiedCheckpointPolicy {
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
            make_coordinator().with_strategy(&make_policy(vec![CheckpointTrigger::AfterExecute]));

        let skipped = coord
            .create_checkpoint_with_strategy(
                CheckpointTrigger::BeforeExecute,
                "loop-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(skipped.is_none());

        let created = coord
            .create_checkpoint_with_strategy(
                CheckpointTrigger::AfterExecute,
                "loop-1",
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
            triggers: vec![CheckpointTrigger::AfterExecute],
            content: None,
            retention: None,
            error_handling: None,
        });

        let result = coord
            .create_checkpoint_with_strategy(
                CheckpointTrigger::AfterExecute,
                "loop-1",
                make_snapshot(),
            )
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn size_budget_truncates_conversation() {
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
        snapshot.conversation_snapshot = Some(vec![
            make_message("m1"),
            make_message("m2"),
            make_message("m3"),
        ]);

        let budget = SizeBudget::new(1024 * 1024, 2);
        let coord = make_coordinator().with_size_budget(budget);

        let ctx = coord
            .prepare("loop-1", CheckpointTrigger::AfterExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        let restored = cp.snapshot.unwrap();
        assert_eq!(
            restored.conversation_snapshot.as_ref().map(|m| m.len()),
            Some(2)
        );
    }

    #[tokio::test]
    async fn restore_migrates_old_format_version() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("loop-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
        cp.format_version = Some("1.0.0".to_string());
        coord.persist(&cp, "loop-1").await.unwrap();

        let entity = coord.restore(&cp.id).await.unwrap();
        assert_eq!(entity.agent_loop_id, "loop-1");
    }

    #[tokio::test]
    async fn restore_rejects_incompatible_version() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("loop-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let mut cp = coord.build(ctx, make_snapshot()).await.unwrap();
        cp.format_version = Some("0.5.0".to_string());
        coord.persist(&cp, "loop-1").await.unwrap();

        let err = coord.restore(&cp.id).await.unwrap_err();
        assert!(matches!(err, CheckpointError::VersionIncompatible { .. }));
    }

    #[tokio::test]
    async fn restore_registers_entity_and_validates_hierarchy() {
        use crate::restore::integrity::InMemoryExecutionRegistry;
        use std::sync::Arc;
        use wf_types::execution::{ChildExecutionReference, ExecutionHierarchy, ExecutionType};

        let storage = Arc::new(StorageBackend::new_memory());
        let sm = AgentCheckpointStateManager::new(storage);
        let registry = Arc::new(InMemoryExecutionRegistry::new());
        let coord = AgentCheckpointCoordinator::new(sm).with_execution_registry(registry.clone());

        let mut snapshot = make_snapshot();
        snapshot.hierarchy = Some(ExecutionHierarchy {
            workflow_id: "wf-1".to_string(),
            execution_id: "loop-1".to_string(),
            parent_execution_id: None,
            depth: 0,
            root_execution_id: None,
            children: Some(vec![ChildExecutionReference {
                child_type: ExecutionType::AgentLoop,
                child_id: "child-loop-1".to_string(),
                created_at: 0,
                fork_path_id: None,
            }]),
        });
        let ctx = coord
            .prepare("loop-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, snapshot).await.unwrap();
        coord.persist(&cp, "loop-1").await.unwrap();

        let entity = coord.restore(&cp.id).await.unwrap();
        assert!(registry.has("loop-1"), "restored entity registered");
        let validation = entity.hierarchy_validation.unwrap();
        assert!(!validation.valid, "orphaned child reference reported");
        assert_eq!(validation.issues.len(), 1);
    }

    #[tokio::test]
    async fn restore_restores_file_checkpoint() {
        use crate::file::{
            FileCheckpointManager, FileCheckpointStorageAdapter, FileState,
            InMemoryFileCheckpointStorage,
        };

        let storage = Arc::new(StorageBackend::new_memory());
        let sm = AgentCheckpointStateManager::new(storage);

        let file_storage = Arc::new(InMemoryFileCheckpointStorage::new());
        let file_manager = FileCheckpointManager::with_storage(file_storage.clone());
        let file_manager2 = FileCheckpointManager::with_storage(file_storage.clone());
        file_manager
            .create_checkpoint(
                "loop-1",
                &[FileState {
                    path: "a.txt".to_string(),
                    hash: "hash_a".to_string(),
                    size: 100,
                    last_modified: 1000,
                }],
            )
            .unwrap();

        let coord = AgentCheckpointCoordinator::new(sm).with_file_checkpoint_manager(file_manager2);

        let ctx = coord
            .prepare("loop-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "loop-1").await.unwrap();

        let entity = coord.restore(&cp.id).await.unwrap();
        assert_eq!(entity.agent_loop_id, "loop-1");
        assert_eq!(
            file_storage.list_by_entity("loop-1", None).unwrap().len(),
            1
        );
    }
}

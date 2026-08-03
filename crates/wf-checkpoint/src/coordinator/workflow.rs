use crate::coordinator::CheckpointCoordinator;
use crate::delta::DeltaRestorer;
use crate::delta::DiffCalculator;
use crate::delta::GenericDeltaRestorer;
use crate::delta::WorkflowDiffCalculator;
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::state::CheckpointStateManager;
use crate::state::WorkflowCheckpoint;
use crate::state::WorkflowCheckpointStateManager;
use std::collections::HashSet;
use std::sync::Arc;
use wf_types::checkpoint::workflow::WorkflowCheckpointDelta;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::CheckpointContext;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::checkpoint::CheckpointType;
use wf_types::checkpoint::DeltaStorageConfig;
use wf_types::storage::CheckpointStorageMetadata;

pub struct WorkflowCheckpointCoordinator {
    state_manager: WorkflowCheckpointStateManager,
    diff_calculator: Arc<dyn DiffCalculator<WorkflowExecutionStateSnapshot, WorkflowCheckpointDelta>>,
    event_bus: Option<CheckpointEventBus>,
    delta_config: DeltaStorageConfig,
}

impl WorkflowCheckpointCoordinator {
    pub fn new(state_manager: WorkflowCheckpointStateManager) -> Self {
        Self {
            state_manager,
            diff_calculator: Arc::new(WorkflowDiffCalculator::new()),
            event_bus: None,
            delta_config: DeltaStorageConfig::default(),
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

    pub fn state_manager(&self) -> &WorkflowCheckpointStateManager {
        &self.state_manager
    }
}

impl CheckpointCoordinator for WorkflowCheckpointCoordinator {
    type Checkpoint = WorkflowCheckpoint;
    type Entity = WorkflowExecutionEntity;
    type State = WorkflowExecutionStateSnapshot;

    async fn prepare(
        &self,
        entity_id: &str,
        _trigger: CheckpointTrigger,
    ) -> Result<CheckpointContext, CheckpointError> {
        Ok(CheckpointContext {
            entity_type: "workflow_execution".to_string(),
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
        state: Self::State,
    ) -> Result<Self::Checkpoint, CheckpointError> {
        let checkpoint_type = self.determine_type(&ctx.entity_id, &self.delta_config).await?;

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
            .save(checkpoint, "workflow_execution", entity_id)
            .await?;

        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::created(checkpoint.id.clone()));
        }

        Ok(())
    }

    async fn restore(&self, checkpoint_id: &str) -> Result<Self::Entity, CheckpointError> {
        let checkpoint = self
            .state_manager
            .load(checkpoint_id)
            .await?
            .ok_or_else(|| CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            })?;

        let entity = match checkpoint.r#type {
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
                })
            }
            None => Err(CheckpointError::Corrupted {
                id: checkpoint_id.to_string(),
                reason: "checkpoint has no type".to_string(),
            }),
        };

        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::restored(
                checkpoint_id.to_string(),
                entity.as_ref().map(|e| e.execution_id.clone()).unwrap_or_default(),
            ));
        }

        entity
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
}

impl WorkflowCheckpointCoordinator {
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

#[derive(Debug, Clone)]
pub struct WorkflowExecutionEntity {
    pub execution_id: String,
    pub status: String,
    pub snapshot: WorkflowExecutionStateSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_storage::backend::StorageBackend;

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
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
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
            attempt: None,
            retry_count: None,
            error: None,
            fallback_used: None,
            metadata: None,
        };
        let checkpoint = coord.build(ctx, make_snapshot()).await.unwrap();
        assert_eq!(checkpoint.r#type, Some(CheckpointType::Full));
        assert!(checkpoint.snapshot.is_some());
    }

    #[tokio::test]
    async fn persist_saves_to_storage() {
        let coord = make_coordinator();
        let ctx = coord
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
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
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
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
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
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
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let ctx = coord
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let ctx = coord
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
            .await
            .unwrap();
        let cp = coord.build(ctx, make_snapshot()).await.unwrap();
        coord.persist(&cp, "exec-1").await.unwrap();

        let ctx = coord
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
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
            .prepare("exec-1", CheckpointTrigger::BeforeExecute)
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
            .prepare("exec-1", CheckpointTrigger::AfterExecute)
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
}

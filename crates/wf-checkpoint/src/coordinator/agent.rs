use crate::coordinator::CheckpointCoordinator;
use crate::delta::AgentDiffCalculator;
use crate::delta::DeltaRestorer;
use crate::delta::DiffCalculator;
use crate::delta::GenericDeltaRestorer;
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::state::AgentCheckpoint;
use crate::state::AgentCheckpointStateManager;
use crate::state::CheckpointStateManager;
use std::collections::HashSet;
use std::sync::Arc;
use wf_types::checkpoint::agent::AgentCheckpointDelta;
use wf_types::checkpoint::agent::AgentStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::CheckpointContext;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::checkpoint::CheckpointType;
use wf_types::checkpoint::DeltaStorageConfig;
use wf_types::storage::CheckpointStorageMetadata;

pub struct AgentCheckpointCoordinator {
    state_manager: AgentCheckpointStateManager,
    diff_calculator: Arc<dyn DiffCalculator<AgentStateSnapshot, AgentCheckpointDelta>>,
    event_bus: Option<CheckpointEventBus>,
    delta_config: DeltaStorageConfig,
}

impl AgentCheckpointCoordinator {
    pub fn new(state_manager: AgentCheckpointStateManager) -> Self {
        Self {
            state_manager,
            diff_calculator: Arc::new(AgentDiffCalculator::new()),
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

    pub fn state_manager(&self) -> &AgentCheckpointStateManager {
        &self.state_manager
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
            .save(checkpoint, "agent_loop", entity_id)
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

                Ok(AgentLoopEntity {
                    agent_loop_id: snapshot.agent_loop_id.clone(),
                    status: snapshot.status.clone(),
                    current_iteration: snapshot.current_iteration,
                    snapshot,
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
                entity
                    .as_ref()
                    .map(|e| e.agent_loop_id.clone())
                    .unwrap_or_default(),
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

#[derive(Debug, Clone)]
pub struct AgentLoopEntity {
    pub agent_loop_id: String,
    pub status: String,
    pub current_iteration: u32,
    pub snapshot: AgentStateSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_storage::backend::StorageBackend;

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
}

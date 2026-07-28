use crate::coordinator::CheckpointCoordinator;
use crate::delta::AgentDiffCalculator;
use crate::delta::DiffCalculator;
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::state::AgentCheckpoint;
use crate::state::AgentCheckpointStateManager;
use crate::state::CheckpointStateManager;
use wf_storage::domain::store::Store;
use wf_types::checkpoint::agent::AgentCheckpointDelta;
use wf_types::checkpoint::agent::AgentStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::CheckpointContext;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::checkpoint::CheckpointType;
use wf_types::checkpoint::DeltaStorageConfig;

pub struct AgentCheckpointCoordinator<S: Store> {
    state_manager: AgentCheckpointStateManager<S>,
    diff_calculator: AgentDiffCalculator,
    event_bus: Option<CheckpointEventBus>,
}

impl<S: Store> AgentCheckpointCoordinator<S> {
    pub fn new(state_manager: AgentCheckpointStateManager<S>) -> Self {
        Self {
            state_manager,
            diff_calculator: AgentDiffCalculator::new(),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn state_manager(&self) -> &AgentCheckpointStateManager<S> {
        &self.state_manager
    }
}

impl<S: Store + Send + Sync> CheckpointCoordinator for AgentCheckpointCoordinator<S> {
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
        let checkpoint_type = if self.should_build_delta(&ctx.entity_id).await? {
            CheckpointType::Delta
        } else {
            CheckpointType::Full
        };

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
                let delta = if let Some(ref prev_meta) = previous {
                    if let Some(prev_cp) = self.state_manager.load(&prev_meta.id).await? {
                        if let Some(ref prev_snapshot) = prev_cp.snapshot {
                            self.diff_calculator
                                .calculate_diff(prev_snapshot, &state)
                                .await?
                        } else {
                            AgentCheckpointDelta {
                                added_messages: state.conversation_snapshot.clone(),
                                added_iterations: Some(vec![state.current_iteration]),
                                status_change: Some(state.status.clone()),
                                other_changes: None,
                            }
                        }
                    } else {
                        return Err(CheckpointError::NotFound {
                            id: prev_meta.id.clone(),
                        });
                    }
                } else {
                    return Err(CheckpointError::DeltaChainBroken {
                        checkpoint_id: ctx.entity_id.clone(),
                        missing_id: "no previous checkpoint for delta".to_string(),
                    });
                };

                Ok(BaseCheckpointCore {
                    id: wf_common::generate_id(),
                    r#type: Some(CheckpointType::Delta),
                    base_checkpoint_id: previous.clone().map(|p| p.id),
                    previous_checkpoint_id: previous.map(|p| p.id),
                    delta: Some(delta),
                    snapshot: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    metadata: None,
                })
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

        match checkpoint.r#type {
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
                let base_id = checkpoint.base_checkpoint_id.as_ref().ok_or_else(|| {
                    CheckpointError::Corrupted {
                        id: checkpoint_id.to_string(),
                        reason: "delta checkpoint missing base_checkpoint_id".to_string(),
                    }
                })?;
                let base_id = base_id.clone();

                let base_checkpoint =
                    self.state_manager.load(&base_id).await?.ok_or_else(|| {
                        CheckpointError::NotFound {
                            id: base_id.clone(),
                        }
                    })?;

                let mut state =
                    base_checkpoint
                        .snapshot
                        .ok_or_else(|| CheckpointError::Corrupted {
                            id: base_id.clone(),
                            reason: "base checkpoint missing snapshot".to_string(),
                        })?;

                let mut current_id = checkpoint.previous_checkpoint_id.clone();
                let mut chain = Vec::new();

                while let Some(id) = current_id {
                    if id == *checkpoint_id {
                        break;
                    }
                    let cp = self
                        .state_manager
                        .load(&id)
                        .await?
                        .ok_or_else(|| CheckpointError::NotFound { id: id.clone() })?;

                    if let Some(ref delta) = cp.delta {
                        chain.push(delta.clone());
                    }
                    current_id = cp.previous_checkpoint_id.clone();
                }

                for delta in chain.iter().rev() {
                    state = self.diff_calculator.apply_delta(&state, delta).await?;
                }

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
        }
    }

    async fn determine_type(
        &self,
        _entity_id: &str,
        config: &DeltaStorageConfig,
    ) -> Result<CheckpointType, CheckpointError> {
        if !config.enabled {
            return Ok(CheckpointType::Full);
        }
        Ok(CheckpointType::Delta)
    }
}

impl<S: Store> AgentCheckpointCoordinator<S> {
    async fn should_build_delta(&self, entity_id: &str) -> Result<bool, CheckpointError> {
        match self.state_manager.get_latest(entity_id).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
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
    use wf_storage::store::memory::MemoryStorage;

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

    fn make_coordinator() -> AgentCheckpointCoordinator<MemoryStorage> {
        let storage = Arc::new(MemoryStorage::new("test"));
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
}

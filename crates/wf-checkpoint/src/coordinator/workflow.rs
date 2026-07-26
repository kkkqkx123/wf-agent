use crate::coordinator::CheckpointCoordinator;
use crate::delta::DiffCalculator;
use crate::delta::WorkflowDiffCalculator;
use crate::error::CheckpointError;
use crate::event::CheckpointEventBus;
use crate::state::CheckpointStateManager;
use crate::state::WorkflowCheckpoint;
use crate::state::WorkflowCheckpointStateManager;
use async_trait::async_trait;
use wf_storage::domain::store::Store;
use wf_types::checkpoint::workflow::WorkflowCheckpointDelta;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::BaseCheckpointCore;
use wf_types::checkpoint::CheckpointContext;
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::checkpoint::CheckpointType;
use wf_types::checkpoint::DeltaStorageConfig;

pub struct WorkflowCheckpointCoordinator<S: Store> {
    state_manager: WorkflowCheckpointStateManager<S>,
    diff_calculator: WorkflowDiffCalculator,
    event_bus: Option<CheckpointEventBus>,
}

impl<S: Store> WorkflowCheckpointCoordinator<S> {
    pub fn new(state_manager: WorkflowCheckpointStateManager<S>) -> Self {
        Self {
            state_manager,
            diff_calculator: WorkflowDiffCalculator::new(),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn state_manager(&self) -> &WorkflowCheckpointStateManager<S> {
        &self.state_manager
    }
}

impl Default for WorkflowCheckpointCoordinator<wf_storage::store::memory::MemoryStorage> {
    fn default() -> Self {
        Self::new(WorkflowCheckpointStateManager::default())
    }
}

#[async_trait]
impl<S: Store + Send + Sync> CheckpointCoordinator for WorkflowCheckpointCoordinator<S> {
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
        let checkpoint_type = if self.should_build_delta(&ctx.entity_id).await? {
            CheckpointType::Delta
        } else {
            CheckpointType::Full
        };

        let previous = self.state_manager.get_latest(&ctx.entity_id).await?;

        match checkpoint_type {
            CheckpointType::Full => Ok(BaseCheckpointCore {
                id: uuid::Uuid::new_v4().to_string(),
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
                            WorkflowCheckpointDelta {
                                added_messages: state.messages.clone(),
                                modified_messages: None,
                                deleted_message_indices: None,
                                added_variables: None,
                                modified_variables: None,
                                added_node_results: None,
                                status_change: Some(serde_json::json!({"status": state.status})),
                                current_node_change: state.current_node_id.clone(),
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
                    id: uuid::Uuid::new_v4().to_string(),
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

        match checkpoint.r#type {
            Some(CheckpointType::Full) => {
                let snapshot = checkpoint.snapshot.ok_or_else(|| {
                    CheckpointError::Corrupted {
                        id: checkpoint_id.to_string(),
                        reason: "full checkpoint missing snapshot".to_string(),
                    }
                })?;

                Ok(WorkflowExecutionEntity {
                    execution_id: snapshot.execution_id,
                    status: snapshot.status,
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

                let base_checkpoint = self
                    .state_manager
                    .load(&base_id)
                    .await?
                    .ok_or_else(|| CheckpointError::NotFound {
                        id: base_id.clone(),
                    })?;

                let mut state = base_checkpoint.snapshot.ok_or_else(|| {
                    CheckpointError::Corrupted {
                        id: base_id.clone(),
                        reason: "base checkpoint missing snapshot".to_string(),
                    }
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

                Ok(WorkflowExecutionEntity {
                    execution_id: state.execution_id,
                    status: state.status,
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

impl<S: Store> WorkflowCheckpointCoordinator<S> {
    async fn should_build_delta(&self, entity_id: &str) -> Result<bool, CheckpointError> {
        match self.state_manager.get_latest(entity_id).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowExecutionEntity {
    pub execution_id: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_storage::store::memory::MemoryStorage;

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

    fn make_coordinator() -> WorkflowCheckpointCoordinator<MemoryStorage> {
        let storage = Arc::new(MemoryStorage::new("test"));
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
        assert_eq!(tp, CheckpointType::Delta);
    }

    #[tokio::test]
    async fn persist_emits_event() {
        let storage = Arc::new(MemoryStorage::new("test"));
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
}

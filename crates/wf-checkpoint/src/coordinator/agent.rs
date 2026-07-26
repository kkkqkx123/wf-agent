use crate::coordinator::CheckpointCoordinator;
use crate::error::CheckpointError;
use async_trait::async_trait;
use wf_types::checkpoint::{
    BaseCheckpointCore, CheckpointContext, CheckpointTrigger, CheckpointType, DeltaStorageConfig,
};
use wf_types::checkpoint::agent::{AgentCheckpointDelta, AgentStateSnapshot};

pub struct AgentCheckpointCoordinator;

impl AgentCheckpointCoordinator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentCheckpointCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CheckpointCoordinator for AgentCheckpointCoordinator {
    type Checkpoint = BaseCheckpointCore<AgentCheckpointDelta, AgentStateSnapshot>;
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
        _ctx: CheckpointContext,
        state: Self::State,
    ) -> Result<Self::Checkpoint, CheckpointError> {
        Ok(BaseCheckpointCore {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: Some(CheckpointType::Full),
            base_checkpoint_id: None,
            previous_checkpoint_id: None,
            delta: None,
            snapshot: Some(state),
            timestamp: chrono::Utc::now().timestamp_millis(),
            metadata: None,
        })
    }

    async fn persist(&self, _checkpoint: &Self::Checkpoint) -> Result<(), CheckpointError> {
        Ok(())
    }

    async fn restore(&self, _checkpoint_id: &str) -> Result<Self::Entity, CheckpointError> {
        Err(CheckpointError::Coordinator(
            "agent restore requires storage integration".to_string(),
        ))
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

#[derive(Debug, Clone)]
pub struct AgentLoopEntity {
    pub agent_loop_id: String,
    pub status: String,
    pub current_iteration: u32,
}

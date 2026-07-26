use crate::coordinator::CheckpointCoordinator;
use crate::error::CheckpointError;
use async_trait::async_trait;
use wf_types::checkpoint::{
    BaseCheckpointCore, CheckpointContext, CheckpointTrigger, CheckpointType, DeltaStorageConfig,
};
use wf_types::checkpoint::workflow::{WorkflowCheckpointDelta, WorkflowExecutionStateSnapshot};

pub struct WorkflowCheckpointCoordinator;

impl WorkflowCheckpointCoordinator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WorkflowCheckpointCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CheckpointCoordinator for WorkflowCheckpointCoordinator {
    type Checkpoint = BaseCheckpointCore<WorkflowCheckpointDelta, WorkflowExecutionStateSnapshot>;
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
            "workflow restore requires storage integration".to_string(),
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
pub struct WorkflowExecutionEntity {
    pub execution_id: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn prepare_returns_context() {
        let coord = WorkflowCheckpointCoordinator::new();
        let ctx = coord.prepare("exec-1", CheckpointTrigger::BeforeExecute).await.unwrap();
        assert_eq!(ctx.entity_type, "workflow_execution");
        assert_eq!(ctx.entity_id, "exec-1");
    }

    #[tokio::test]
    async fn build_creates_full_checkpoint() {
        let coord = WorkflowCheckpointCoordinator::new();
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
    async fn determine_type_respects_config() {
        let coord = WorkflowCheckpointCoordinator::new();
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
        let tp = coord.determine_type("exec-1", &config_enabled).await.unwrap();
        assert_eq!(tp, CheckpointType::Delta);
    }
}

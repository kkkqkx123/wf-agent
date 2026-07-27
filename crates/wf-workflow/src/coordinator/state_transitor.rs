use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_types::Id;

use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;

pub struct WorkflowStateTransitor;

impl WorkflowStateTransitor {
    pub async fn start_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.start();
        Ok(())
    }

    pub async fn pause_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.pause();
        Ok(())
    }

    pub async fn resume_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.resume();
        Ok(())
    }

    pub async fn complete_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.complete();
        Ok(())
    }

    pub async fn fail_workflow_execution(entity: &WorkflowExecutionEntity, error: String) -> WorkflowResult<()> {
        entity.state.write().await.fail(error);
        Ok(())
    }

    pub async fn cancel_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.cancel();
        Ok(())
    }
}

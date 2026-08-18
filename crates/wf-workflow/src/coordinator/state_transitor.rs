use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;

pub struct WorkflowStateTransitor;

impl WorkflowStateTransitor {
    pub async fn start_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.start()
    }

    pub async fn pause_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.pause()
    }

    pub async fn resume_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.resume()
    }

    pub async fn complete_workflow_execution(
        entity: &WorkflowExecutionEntity,
    ) -> WorkflowResult<()> {
        entity.state.write().await.complete()
    }

    pub async fn fail_workflow_execution(
        entity: &WorkflowExecutionEntity,
        error: String,
    ) -> WorkflowResult<()> {
        entity.state.write().await.fail(error)
    }

    pub async fn cancel_workflow_execution(entity: &WorkflowExecutionEntity) -> WorkflowResult<()> {
        entity.state.write().await.cancel()
    }
}

use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;

pub struct WorkflowExecutionBuilder;

impl WorkflowExecutionBuilder {
    pub fn build(
        _id: wf_types::Id,
        _workflow_id: wf_types::Id,
    ) -> WorkflowResult<WorkflowExecutionEntity> {
        Ok(WorkflowExecutionEntity::new(
            wf_types::Id::new(),
            wf_types::Id::new(),
        ))
    }
}

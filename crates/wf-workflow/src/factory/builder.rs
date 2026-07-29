use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;

pub struct WorkflowExecutionBuilder {
    id: Option<wf_types::Id>,
    workflow_id: Option<wf_types::Id>,
    parent_execution_id: Option<wf_types::Id>,
}

impl Default for WorkflowExecutionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowExecutionBuilder {
    pub fn new() -> Self {
        Self {
            id: None,
            workflow_id: None,
            parent_execution_id: None,
        }
    }

    pub fn with_id(mut self, id: wf_types::Id) -> Self {
        self.id = Some(id);
        self
    }

    pub fn with_workflow_id(mut self, workflow_id: wf_types::Id) -> Self {
        self.workflow_id = Some(workflow_id);
        self
    }

    pub fn with_parent_execution_id(mut self, parent_id: wf_types::Id) -> Self {
        self.parent_execution_id = Some(parent_id);
        self
    }

    pub fn build(self) -> WorkflowResult<WorkflowExecutionEntity> {
        let id = self.id.unwrap_or_default();
        let workflow_id = self.workflow_id.unwrap_or_default();

        let entity = WorkflowExecutionEntity::new(id, workflow_id);

        let entity = if let Some(parent_id) = self.parent_execution_id {
            entity.with_parent_execution_id(parent_id)
        } else {
            entity
        };

        Ok(entity)
    }
}

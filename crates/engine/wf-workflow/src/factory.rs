use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;

pub struct WorkflowExecutionBuilder {
    id: Option<wf_types::Id>,
    workflow_id: Option<wf_types::Id>,
    hierarchy_manager:
        Option<std::sync::Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>>,
    execution_type: Option<wf_types::workflow_execution::WorkflowExecutionType>,
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
            hierarchy_manager: None,
            execution_type: None,
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

    pub fn with_hierarchy_manager(
        mut self,
        manager: std::sync::Arc<wf_core::hierarchy::manager::ExecutionHierarchyManager>,
    ) -> Self {
        self.hierarchy_manager = Some(manager);
        self
    }

    pub fn with_execution_type(
        mut self,
        execution_type: wf_types::workflow_execution::WorkflowExecutionType,
    ) -> Self {
        self.execution_type = Some(execution_type);
        self
    }

    pub fn build(self) -> WorkflowResult<WorkflowExecutionEntity> {
        let id = self.id.unwrap_or_default();
        let workflow_id = self.workflow_id.unwrap_or_default();

        let mut entity = WorkflowExecutionEntity::new(id, workflow_id);

        if let Some(manager) = self.hierarchy_manager {
            entity = entity.with_hierarchy_manager(manager);
        }
        if let Some(execution_type) = self.execution_type {
            entity = entity.with_execution_type(execution_type);
        }

        Ok(entity)
    }
}

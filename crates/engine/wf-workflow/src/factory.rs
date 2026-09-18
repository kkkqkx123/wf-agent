use crate::entity::WorkflowExecutionEntity;
use crate::error::WorkflowResult;

pub struct WorkflowExecutionBuilder {
    id: Option<wf_types::Id>,
    workflow_id: Option<wf_types::Id>,
    parent_execution_id: Option<wf_types::Id>,
    ancestors: Vec<wf_types::Id>,
    hierarchy_depth: Option<u32>,
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
            ancestors: Vec::new(),
            hierarchy_depth: None,
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

    pub fn with_ancestors(mut self, ancestors: Vec<wf_types::Id>) -> Self {
        self.ancestors = ancestors;
        self
    }

    pub fn with_hierarchy_depth(mut self, depth: u32) -> Self {
        self.hierarchy_depth = Some(depth);
        self
    }

    pub fn build(self) -> WorkflowResult<WorkflowExecutionEntity> {
        let id = self.id.unwrap_or_default();
        let workflow_id = self.workflow_id.unwrap_or_default();

        let mut entity = WorkflowExecutionEntity::new(id, workflow_id);

        if let Some(parent_id) = self.parent_execution_id {
            entity = entity.with_parent_execution_id(parent_id);
        }
        if !self.ancestors.is_empty() {
            entity = entity.with_ancestors(self.ancestors);
        }
        if let Some(depth) = self.hierarchy_depth {
            entity = entity.with_hierarchy_depth(depth);
        }

        Ok(entity)
    }
}

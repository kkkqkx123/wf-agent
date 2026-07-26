use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct WorkflowExecutionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
    pub workflow_id_filter: Option<String>,
}

pub trait WorkflowExecutionStorageAdapter:
    BaseStorageAdapter<wf_types::WorkflowExecution, WorkflowExecutionListOptions>
{
    async fn update_status(
        &self,
        id: &str,
        status: &wf_types::ExecutionStatus,
    ) -> Result<(), StorageError>;
}

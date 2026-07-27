use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct WorkflowExecutionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
    pub workflow_id_filter: Option<String>,
}

impl From<WorkflowExecutionListOptions> for QueryFilter {
    fn from(opts: WorkflowExecutionListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(status) = opts.status_filter {
            filter.status = Some(status);
        }
        if let Some(workflow_id) = opts.workflow_id_filter {
            filter.fields.insert("workflowId".to_string(), workflow_id);
        }
        filter
    }
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

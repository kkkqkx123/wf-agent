use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
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
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.status_filter {
            filter.add_op(FilterOp::Eq("status".into(), value));
        }
        if let Some(value) = opts.workflow_id_filter {
            filter.add_op(FilterOp::Eq("workflowId".into(), value));
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

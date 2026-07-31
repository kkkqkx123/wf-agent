use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct TaskListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
    pub task_type_filter: Option<String>,
}

impl From<TaskListOptions> for QueryFilter {
    fn from(opts: TaskListOptions) -> Self {
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
        if let Some(value) = opts.task_type_filter {
            filter.add_op(FilterOp::Eq("taskType".into(), value));
        }
        filter
    }
}

pub trait TaskStorageAdapter:
    BaseStorageAdapter<wf_types::TaskStorageMetadata, TaskListOptions>
{
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError>;
    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError>;
}

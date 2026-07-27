use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::QueryFilter;
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
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(status) = opts.status_filter {
            filter.status = Some(status);
        }
        if let Some(task_type) = opts.task_type_filter {
            filter.fields.insert("taskType".to_string(), task_type);
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

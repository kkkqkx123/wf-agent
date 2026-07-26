use std::collections::HashMap;
use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct TaskListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
    pub task_type_filter: Option<String>,
}

pub trait TaskStorageAdapter:
    BaseStorageAdapter<wf_types::TaskStorageMetadata, TaskListOptions>
{
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError>;
    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError>;
}

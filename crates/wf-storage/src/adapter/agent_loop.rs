use std::collections::HashMap;
use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct AgentLoopListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
}

pub trait AgentLoopStorageAdapter:
    BaseStorageAdapter<wf_types::AgentLoopStorageMetadata, AgentLoopListOptions>
{
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError>;

    async fn list_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<wf_types::AgentLoopStorageMetadata>, StorageError>;

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError>;
}

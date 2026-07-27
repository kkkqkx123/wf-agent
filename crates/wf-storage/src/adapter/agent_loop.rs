use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct AgentLoopListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
}

impl From<AgentLoopListOptions> for QueryFilter {
    fn from(opts: AgentLoopListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(status) = opts.status_filter {
            filter.status = Some(status);
        }
        filter
    }
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

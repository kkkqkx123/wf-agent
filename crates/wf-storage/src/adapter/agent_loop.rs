use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
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

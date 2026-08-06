use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::collections::HashMap;
use std::future::Future;

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
    fn update_status<'a>(
        &'a self,
        id: &'a str,
        status: &'a str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;

    fn list_by_status<'a>(
        &'a self,
        status: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::AgentLoopStorageMetadata>, StorageError>> + Send + 'a;

    fn get_stats<'a>(
        &'a self,
    ) -> impl Future<Output = Result<HashMap<String, u64>, StorageError>> + Send + 'a;
}

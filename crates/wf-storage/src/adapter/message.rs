use std::collections::HashMap;
use std::future::Future;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct MessageListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub execution_id_filter: Option<String>,
    pub agent_loop_id_filter: Option<String>,
    pub role_filter: Option<String>,
}

impl From<MessageListOptions> for QueryFilter {
    fn from(opts: MessageListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.execution_id_filter {
            filter.add_op(FilterOp::Eq("executionId".into(), value));
        }
        if let Some(value) = opts.agent_loop_id_filter {
            filter.add_op(FilterOp::Eq("agentLoopId".into(), value));
        }
        if let Some(value) = opts.role_filter {
            filter.add_op(FilterOp::Eq("role".into(), value));
        }
        filter
    }
}

pub trait MessageStorageAdapter:
    BaseStorageAdapter<wf_types::MessageStorageMetadata, MessageListOptions>
{
    fn list_by_execution<'a>(
        &'a self,
        execution_id: &'a str,
        options: Option<MessageListOptions>,
    ) -> impl Future<Output = Result<Vec<wf_types::MessageStorageMetadata>, StorageError>> + Send + 'a;

    fn list_by_agent_loop<'a>(
        &'a self,
        agent_loop_id: &'a str,
        options: Option<MessageListOptions>,
    ) -> impl Future<Output = Result<Vec<wf_types::MessageStorageMetadata>, StorageError>> + Send + 'a;

    /// Message count grouped by role.
    fn get_stats<'a>(
        &'a self,
    ) -> impl Future<Output = Result<HashMap<String, u64>, StorageError>> + Send + 'a;
}

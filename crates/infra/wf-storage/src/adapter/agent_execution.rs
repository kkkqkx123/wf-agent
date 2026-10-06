use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::future::Future;

#[derive(Debug, Clone, Default)]
pub struct AgentExecutionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub definition_id_filter: Option<String>,
    pub status_filter: Option<String>,
    /// Inclusive lower bound on `startedAt` (ms epoch).
    pub started_from: Option<i64>,
    /// Inclusive upper bound on `startedAt` (ms epoch).
    pub started_to: Option<i64>,
    /// Sort by execution start time; true sorts newest first.
    /// Absent preserves backend default order.
    pub order_desc: Option<bool>,
}

impl From<AgentExecutionListOptions> for QueryFilter {
    fn from(opts: AgentExecutionListOptions) -> Self {
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
        if let Some(value) = opts.definition_id_filter {
            filter.add_op(FilterOp::Eq("definitionId".into(), value));
        }
        match (opts.started_from, opts.started_to) {
            (Some(from), Some(to)) => {
                filter.add_op(FilterOp::Between("startedAt".into(), from, to));
            }
            (Some(from), None) => {
                if from > i64::MIN {
                    filter.add_op(FilterOp::Gt("startedAt".into(), from - 1));
                }
            }
            (None, Some(to)) => {
                if to < i64::MAX {
                    filter.add_op(FilterOp::Lt("startedAt".into(), to + 1));
                }
            }
            (None, None) => {}
        }
        if let Some(descending) = opts.order_desc {
            filter.add_op(FilterOp::OrderBy("startedAt".into(), descending));
        }
        filter
    }
}

pub trait AgentExecutionStorageAdapter:
    BaseStorageAdapter<wf_types::AgentExecution, AgentExecutionListOptions>
{
    fn list_by_definition<'a>(
        &'a self,
        definition_id: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::AgentExecution>, StorageError>> + Send + 'a;

    /// Update only the status field of a persisted agent execution, avoiding
    /// a full record read-modify-write.
    fn update_status<'a>(
        &'a self,
        id: &'a str,
        status: &'a wf_types::ExecutionStatus,
    ) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;
}

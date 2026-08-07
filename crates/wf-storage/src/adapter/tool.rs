use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::collections::HashMap;
use std::future::Future;

#[derive(Debug, Clone, Default)]
pub struct ToolListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub tool_type_filter: Option<String>,
}

impl From<ToolListOptions> for QueryFilter {
    fn from(opts: ToolListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.tool_type_filter {
            filter.add_op(FilterOp::Eq("toolType".into(), value));
        }
        filter
    }
}

pub trait ToolStorageAdapter:
    BaseStorageAdapter<wf_types::ToolStorageMetadata, ToolListOptions>
{
    fn get_stats<'a>(
        &'a self,
    ) -> impl Future<Output = Result<HashMap<String, u64>, StorageError>> + Send + 'a;

    /// Atomically set the enabled flag of a tool (compare-and-set read /
    /// modify / write guarded against lost updates). Returns the updated
    /// record, or `None` when no tool with the id exists.
    fn set_enabled<'a>(
        &'a self,
        id: &'a str,
        enabled: bool,
    ) -> impl Future<Output = Result<Option<wf_types::ToolStorageMetadata>, StorageError>> + Send + 'a;
}

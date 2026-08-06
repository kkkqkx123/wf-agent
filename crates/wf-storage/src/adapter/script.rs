use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::future::Future;

#[derive(Debug, Clone, Default)]
pub struct ScriptListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub language_filter: Option<String>,
}

impl From<ScriptListOptions> for QueryFilter {
    fn from(opts: ScriptListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.language_filter {
            filter.add_op(FilterOp::Eq("language".into(), value));
        }
        filter
    }
}

pub trait ScriptStorageAdapter:
    BaseStorageAdapter<wf_types::ScriptStorageMetadata, ScriptListOptions>
{
    fn list_by_language<'a>(
        &'a self,
        language: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::ScriptStorageMetadata>, StorageError>> + Send + 'a;
}

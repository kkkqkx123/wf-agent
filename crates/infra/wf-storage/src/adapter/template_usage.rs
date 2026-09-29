use std::future::Future;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct TemplateUsageListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub kind_filter: Option<String>,
}

impl From<TemplateUsageListOptions> for QueryFilter {
    fn from(opts: TemplateUsageListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.kind_filter {
            filter.add_op(FilterOp::Eq("kind".into(), value));
        }
        filter
    }
}

pub trait TemplateUsageStorageAdapter:
    BaseStorageAdapter<wf_types::TemplateUsageMetadata, TemplateUsageListOptions>
{
    /// Increment the counter of a template, creating the record on first
    /// use. Returns the counter after the increment.
    fn increment<'a>(
        &'a self,
        template_id: &'a str,
        kind: &'a str,
    ) -> impl Future<Output = Result<u64, StorageError>> + Send + 'a;

    /// Current counter of a template; zero when never recorded.
    fn get_count<'a>(
        &'a self,
        template_id: &'a str,
    ) -> impl Future<Output = Result<u64, StorageError>> + Send + 'a;
}

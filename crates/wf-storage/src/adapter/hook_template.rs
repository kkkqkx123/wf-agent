use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::future::Future;

#[derive(Debug, Clone, Default)]
pub struct HookTemplateListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub hook_type_filter: Option<String>,
}

impl From<HookTemplateListOptions> for QueryFilter {
    fn from(opts: HookTemplateListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.hook_type_filter {
            filter.add_op(FilterOp::Eq("hookType".into(), value));
        }
        filter
    }
}

pub trait HookTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::HookTemplateStorageMetadata, HookTemplateListOptions>
{
    fn list_by_hook_type<'a>(
        &'a self,
        hook_type: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::HookTemplateStorageMetadata>, StorageError>> + Send + 'a;
}

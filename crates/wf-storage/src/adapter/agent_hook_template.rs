use std::future::Future;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct AgentHookTemplateListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub hook_type_filter: Option<String>,
    pub name_filter: Option<String>,
    pub category_filter: Option<String>,
}

impl From<AgentHookTemplateListOptions> for QueryFilter {
    fn from(opts: AgentHookTemplateListOptions) -> Self {
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
        if let Some(value) = opts.name_filter {
            filter.add_op(FilterOp::Eq("name".into(), value));
        }
        if let Some(value) = opts.category_filter {
            filter.add_op(FilterOp::Eq("category".into(), value));
        }
        filter
    }
}

pub trait AgentHookTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::AgentHookTemplateStorageMetadata, AgentHookTemplateListOptions>
{
    fn list_by_hook_type<'a>(
        &'a self,
        hook_type: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::AgentHookTemplateStorageMetadata>, StorageError>>
           + Send
           + 'a;

    fn list_by_category<'a>(
        &'a self,
        category: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::AgentHookTemplateStorageMetadata>, StorageError>>
           + Send
           + 'a;
}

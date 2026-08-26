use std::future::Future;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct TriggerTemplateListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub trigger_type_filter: Option<String>,
    pub name_filter: Option<String>,
    pub category_filter: Option<String>,
    pub enabled_filter: Option<bool>,
}

impl From<TriggerTemplateListOptions> for QueryFilter {
    fn from(opts: TriggerTemplateListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.trigger_type_filter {
            filter.add_op(FilterOp::Eq("triggerType".into(), value));
        }
        if let Some(value) = opts.name_filter {
            filter.add_op(FilterOp::Eq("name".into(), value));
        }
        if let Some(value) = opts.category_filter {
            filter.add_op(FilterOp::Eq("category".into(), value));
        }
        if let Some(value) = opts.enabled_filter {
            filter.add_op(FilterOp::Eq("enabled".into(), value.to_string()));
        }
        filter
    }
}

pub trait TriggerTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::TriggerTemplateStorageMetadata, TriggerTemplateListOptions>
{
    fn list_by_trigger_type<'a>(
        &'a self,
        trigger_type: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::TriggerTemplateStorageMetadata>, StorageError>>
           + Send
           + 'a;

    fn list_by_category<'a>(
        &'a self,
        category: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::TriggerTemplateStorageMetadata>, StorageError>>
           + Send
           + 'a;
}

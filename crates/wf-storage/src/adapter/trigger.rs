use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct TriggerListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub event_filter: Option<String>,
    pub enabled_filter: Option<bool>,
}

impl From<TriggerListOptions> for QueryFilter {
    fn from(opts: TriggerListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.event_filter {
            filter.add_op(FilterOp::Eq("event".into(), value));
        }
        if let Some(value) = opts.enabled_filter {
            filter.add_op(FilterOp::Eq("enabled".into(), value.to_string()));
        }
        filter
    }
}

pub trait TriggerStorageAdapter:
    BaseStorageAdapter<wf_types::TriggerStorageMetadata, TriggerListOptions>
{
    async fn list_by_event(
        &self,
        event: &str,
    ) -> Result<Vec<wf_types::TriggerStorageMetadata>, StorageError>;
}

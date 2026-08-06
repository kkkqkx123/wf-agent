use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::future::Future;

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
    fn list_by_event<'a>(
        &'a self,
        event: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::TriggerStorageMetadata>, StorageError>> + Send + 'a;

    /// Atomically set the enabled flag of a trigger (compare-and-set read /
    /// modify / write guarded against lost updates). Returns the updated
    /// record, or `None` when no trigger with the id exists.
    fn set_enabled<'a>(
        &'a self,
        id: &'a str,
        enabled: bool,
    ) -> impl Future<Output = Result<Option<wf_types::TriggerStorageMetadata>, StorageError>> + Send + 'a;
}

use crate::domain::store::QueryFilter;
use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct TriggerListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub event_filter: Option<String>,
    pub enabled_filter: Option<bool>,
}

impl From<TriggerListOptions> for QueryFilter {
    fn from(opts: TriggerListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(event) = opts.event_filter {
            filter.fields.insert("event".to_string(), event);
        }
        if let Some(enabled) = opts.enabled_filter {
            filter.fields.insert("enabled".to_string(), enabled.to_string());
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

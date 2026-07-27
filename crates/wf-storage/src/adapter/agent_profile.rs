use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct AgentProfileListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub name_filter: Option<String>,
    pub is_default: Option<bool>,
}

impl From<AgentProfileListOptions> for QueryFilter {
    fn from(opts: AgentProfileListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(name) = opts.name_filter {
            filter.fields.insert("name".to_string(), name);
        }
        if let Some(is_default) = opts.is_default {
            filter
                .fields
                .insert("isDefault".to_string(), is_default.to_string());
        }
        filter
    }
}

pub trait AgentProfileStorageAdapter:
    BaseStorageAdapter<wf_types::AgentProfileStorageMetadata, AgentProfileListOptions>
{
    async fn get_first(
        &self,
    ) -> Result<Option<wf_types::AgentProfileStorageMetadata>, StorageError>;
}

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
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
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.name_filter {
            filter.add_op(FilterOp::Eq("name".into(), value));
        }
        if let Some(value) = opts.is_default {
            filter.add_op(FilterOp::Eq("isDefault".into(), value.to_string()));
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

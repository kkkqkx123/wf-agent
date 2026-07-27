use std::collections::HashMap;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct ToolListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub tool_type_filter: Option<String>,
}

impl From<ToolListOptions> for QueryFilter {
    fn from(opts: ToolListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(typ) = opts.tool_type_filter {
            filter.fields.insert("toolType".to_string(), typ);
        }
        filter
    }
}

pub trait ToolStorageAdapter:
    BaseStorageAdapter<wf_types::ToolStorageMetadata, ToolListOptions>
{
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError>;
}

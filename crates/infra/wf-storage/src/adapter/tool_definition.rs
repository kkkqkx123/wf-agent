use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::future::Future;

#[derive(Debug, Clone, Default)]
pub struct ToolDefinitionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub tool_type_filter: Option<String>,
}

impl From<ToolDefinitionListOptions> for QueryFilter {
    fn from(opts: ToolDefinitionListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.tool_type_filter {
            filter.add_op(FilterOp::Eq("toolType".into(), value));
        }
        filter
    }
}

/// Persistence of full tool definitions (payload carries parameters and
/// config) so runtime-registered tools can be restored into the tool
/// registry after a restart. Distinct from the metadata-only
/// `ToolStorageAdapter` (`ToolStorageMetadata`), which serves API/audit
/// views.
pub trait ToolDefinitionStorageAdapter:
    BaseStorageAdapter<wf_types::tool::Tool, ToolDefinitionListOptions>
{
    fn list_by_tool_type<'a>(
        &'a self,
        tool_type: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::tool::Tool>, StorageError>> + Send + 'a;
}

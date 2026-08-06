use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::future::Future;

#[derive(Debug, Clone, Default)]
pub struct NodeTemplateListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub node_type_filter: Option<String>,
}

impl From<NodeTemplateListOptions> for QueryFilter {
    fn from(opts: NodeTemplateListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.node_type_filter {
            filter.add_op(FilterOp::Eq("nodeType".into(), value));
        }
        filter
    }
}

pub trait NodeTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::NodeTemplateStorageMetadata, NodeTemplateListOptions>
{
    fn list_by_node_type<'a>(
        &'a self,
        node_type: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::NodeTemplateStorageMetadata>, StorageError>> + Send + 'a;
}

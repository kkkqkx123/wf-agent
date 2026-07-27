use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct NodeTemplateListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub node_type_filter: Option<String>,
}

impl From<NodeTemplateListOptions> for QueryFilter {
    fn from(opts: NodeTemplateListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(typ) = opts.node_type_filter {
            filter.fields.insert("nodeType".to_string(), typ);
        }
        filter
    }
}

pub trait NodeTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::NodeTemplateStorageMetadata, NodeTemplateListOptions>
{
    async fn list_by_node_type(
        &self,
        node_type: &str,
    ) -> Result<Vec<wf_types::NodeTemplateStorageMetadata>, StorageError>;
}

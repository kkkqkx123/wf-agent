use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct FileCheckpointListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub entity_id_filter: Option<String>,
}

impl From<FileCheckpointListOptions> for QueryFilter {
    fn from(opts: FileCheckpointListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.entity_id_filter {
            filter.add_op(FilterOp::Eq("entityId".into(), value));
        }
        filter
    }
}

pub trait FileCheckpointStorageAdapter:
    BaseStorageAdapter<wf_types::FileCheckpointStorageMetadata, FileCheckpointListOptions>
{
    async fn load_by_file_path(
        &self,
        file_path: &str,
    ) -> Result<Option<wf_types::FileCheckpointStorageMetadata>, StorageError>;

    async fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> Result<Vec<wf_types::FileCheckpointStorageMetadata>, StorageError>;
}

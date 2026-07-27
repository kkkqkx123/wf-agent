use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct FileCheckpointListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub entity_id_filter: Option<String>,
}

impl From<FileCheckpointListOptions> for QueryFilter {
    fn from(opts: FileCheckpointListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(eid) = opts.entity_id_filter {
            filter.fields.insert("entityId".to_string(), eid);
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

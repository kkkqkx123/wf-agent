use crate::error::StorageError;
use crate::adapter::base::{BaseStorageAdapter, ListOptions};

pub trait FileCheckpointStorageAdapter:
    BaseStorageAdapter<wf_types::FileCheckpointStorageMetadata, ListOptions>
{
    async fn load_by_file_path(
        &self,
        file_path: &str,
    ) -> Result<Option<wf_types::FileCheckpointStorageMetadata>, StorageError>;
}

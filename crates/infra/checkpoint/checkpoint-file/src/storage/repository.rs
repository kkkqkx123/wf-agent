use checkpoint_base::error::CheckpointError;

pub type StorageResult<T> = Result<T, CheckpointError>;

pub trait MetadataStore {
    fn store_metadata(&self, key: &str, value: &str) -> StorageResult<()>;
    fn load_metadata(&self, key: &str) -> StorageResult<Option<String>>;
}

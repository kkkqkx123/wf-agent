use checkpoint_base::error::CheckpointError;

pub type StorageResult<T> = Result<T, CheckpointError>;

pub trait AtomicOps {
    fn with_atomic<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&Self) -> StorageResult<T>,
    {
        f(self)
    }
}

pub trait MetadataStore {
    fn store_metadata(&self, key: &str, value: &str) -> StorageResult<()>;
    fn load_metadata(&self, key: &str) -> StorageResult<Option<String>>;
    fn delete_metadata(&self, key: &str) -> StorageResult<bool>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphBlob {
    pub id: String,
    pub data: Vec<u8>,
    pub parent_id: Option<String>,
    pub branch_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub trait GraphBlobStore: Send + Sync {
    fn store_graph_blob(
        &self,
        id: &str,
        data: &[u8],
        parent_id: Option<&str>,
        branch_id: Option<&str>,
    ) -> StorageResult<()>;
    fn load_graph_blob(&self, id: &str) -> StorageResult<Option<GraphBlob>>;
    fn delete_graph_blob(&self, id: &str) -> StorageResult<bool>;
    fn list_graph_blob_ids(&self, parent_id: Option<&str>) -> StorageResult<Vec<String>>;
    fn list_graph_blob_ids_by_branch(&self, branch_id: &str) -> StorageResult<Vec<String>>;
}

pub trait Repository: AtomicOps {}

use crate::domain::store::QueryFilter;
use crate::error::StorageError;

impl From<ListOptions> for QueryFilter {
    fn from(opts: ListOptions) -> Self {
        Self {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        }
    }
}

pub trait BaseStorageAdapter<TEntity, TListOptions>: Send + Sync {
    async fn initialize(&self) -> Result<(), StorageError>;
    async fn close(&self) -> Result<(), StorageError>;

    async fn save(&self, entity: &TEntity) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Option<TEntity>, StorageError>;
    async fn delete(&self, id: &str) -> Result<bool, StorageError>;
    async fn list(&self, options: Option<TListOptions>) -> Result<Vec<TEntity>, StorageError>;
    async fn clear(&self) -> Result<(), StorageError>;

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        Ok(self.load(id).await?.is_some())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

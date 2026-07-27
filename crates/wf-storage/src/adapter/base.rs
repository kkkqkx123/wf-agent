use serde::Serialize;

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

    async fn save_batch(&self, entities: &[TEntity]) -> Result<(), StorageError>
    where
        TEntity: Serialize,
    {
        for entity in entities {
            self.save(entity).await?;
        }
        Ok(())
    }

    async fn load_batch(&self, ids: &[String]) -> Result<Vec<(String, TEntity)>, StorageError> {
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(entity) = self.load(id).await? {
                results.push((id.clone(), entity));
            }
        }
        Ok(results)
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<u64, StorageError> {
        let mut count = 0u64;
        for id in ids {
            if self.delete(id).await? {
                count += 1;
            }
        }
        Ok(count)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

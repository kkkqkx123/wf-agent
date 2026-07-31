use serde::Serialize;
use std::collections::HashMap;

use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;

impl From<ListOptions> for QueryFilter {
    fn from(opts: ListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        filter
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

    async fn count_by_field(&self, field: &str) -> Result<HashMap<String, u64>, StorageError>;

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

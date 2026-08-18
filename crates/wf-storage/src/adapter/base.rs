use std::collections::HashMap;
use std::future::Future;

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

pub trait BaseStorageAdapter<TEntity, TListOptions>: Send + Sync
where
    TEntity: Send + Sync,
    TListOptions: Send + Sync,
{
    fn initialize<'a>(&'a self) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;

    fn close<'a>(&'a self) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;

    fn save<'a>(
        &'a self,
        entity: &'a TEntity,
    ) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;

    fn load<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Future<Output = Result<Option<TEntity>, StorageError>> + Send + 'a;

    fn delete<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Future<Output = Result<bool, StorageError>> + Send + 'a;

    fn list<'a>(
        &'a self,
        options: Option<TListOptions>,
    ) -> impl Future<Output = Result<Vec<TEntity>, StorageError>> + Send + 'a;

    fn clear<'a>(&'a self) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;

    fn exists<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Future<Output = Result<bool, StorageError>> + Send + 'a {
        async move { Ok(self.load(id).await?.is_some()) }
    }

    fn count_by_field<'a>(
        &'a self,
        field: &'a str,
    ) -> impl Future<Output = Result<HashMap<String, u64>, StorageError>> + Send + 'a;

    fn save_batch<'a>(
        &'a self,
        entities: &'a [TEntity],
    ) -> impl Future<Output = Result<(), StorageError>> + Send + 'a {
        async move {
            for entity in entities {
                self.save(entity).await?;
            }
            Ok(())
        }
    }

    fn load_batch<'a>(
        &'a self,
        ids: &'a [String],
    ) -> impl Future<Output = Result<Vec<(String, TEntity)>, StorageError>> + Send + 'a {
        async move {
            let mut results = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(entity) = self.load(id).await? {
                    results.push((id.clone(), entity));
                }
            }
            Ok(results)
        }
    }

    fn delete_batch<'a>(
        &'a self,
        ids: &'a [String],
    ) -> impl Future<Output = Result<u64, StorageError>> + Send + 'a {
        async move {
            let mut count = 0u64;
            for id in ids {
                if self.delete(id).await? {
                    count += 1;
                }
            }
            Ok(count)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

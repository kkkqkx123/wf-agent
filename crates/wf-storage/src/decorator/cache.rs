use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use moka::sync::Cache;
use serde_json::Value;

use crate::domain::store::{
    BatchItem, BatchStore, Maintainable, QueryFilter, Store, StoreOperation,
};
use crate::error::StorageError;

pub struct CacheConfig {
    pub max_capacity: u64,
    pub ttl_seconds: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 1000,
            ttl_seconds: 300,
        }
    }
}

#[derive(Debug, Clone)]
struct CachedEntry {
    data: Vec<u8>,
    metadata: Value,
}

#[derive(Debug)]
pub struct EntityCache {
    cache: Cache<String, CachedEntry>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl EntityCache {
    pub fn new(config: CacheConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(std::time::Duration::from_secs(config.ttl_seconds))
            .build();
        Self {
            cache,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    pub fn get(&self, id: &str) -> Option<(Vec<u8>, Value)> {
        match self.cache.get(id) {
            Some(entry) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some((entry.data, entry.metadata))
            }
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    pub fn insert(&self, id: String, data: Vec<u8>, metadata: Value) {
        self.cache.insert(id, CachedEntry { data, metadata });
    }

    pub fn invalidate(&self, id: &str) {
        self.cache.invalidate(id);
    }

    pub fn clear(&self) {
        self.cache.invalidate_all();
    }

    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    pub fn hit_rate(&self) -> f64 {
        let h = self.hits();
        let m = self.misses();
        let total = h + m;
        if total == 0 {
            0.0
        } else {
            h as f64 / total as f64
        }
    }

    pub fn entry_count(&self) -> u64 {
        self.cache.entry_count()
    }
}

#[derive(Debug, Clone)]
pub struct CachingStore<S> {
    inner: S,
    cache: Arc<EntityCache>,
}

impl<S: Store> CachingStore<S> {
    pub fn new(inner: S, cache_config: CacheConfig) -> Self {
        Self {
            inner,
            cache: Arc::new(EntityCache::new(cache_config)),
        }
    }

    pub fn cache(&self) -> &EntityCache {
        &self.cache
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

#[async_trait]
impl<S: Store> Store for CachingStore<S> {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        self.inner.save(id, data, metadata).await?;
        self.cache.invalidate(id);
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        if let Some(entry) = self.cache.get(id) {
            return Ok(Some(entry));
        }
        match self.inner.load(id).await? {
            Some(entry) => {
                self.cache
                    .insert(id.to_string(), entry.0.clone(), entry.1.clone());
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        self.inner.delete(id).await?;
        self.cache.invalidate(id);
        Ok(())
    }

    async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        self.inner.list(filter).await
    }

    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        self.inner.list_data(filter).await
    }

    async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        self.inner.count(filter).await
    }

    async fn count_by_metadata_field(
        &self,
        field: &str,
    ) -> Result<std::collections::HashMap<String, u64>, StorageError> {
        self.inner.count_by_metadata_field(field).await
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        if self.cache.get(id).is_some() {
            return Ok(true);
        }
        self.inner.exists(id).await
    }

    async fn clear(&self) -> Result<(), StorageError> {
        self.inner.clear().await?;
        self.cache.clear();
        Ok(())
    }

    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        self.inner.update_status(id, status).await?;
        self.cache.invalidate(id);
        Ok(())
    }

    async fn apply_batch(&self, operations: &[StoreOperation]) -> Result<(), StorageError> {
        self.inner.apply_batch(operations).await?;
        for operation in operations {
            match operation {
                StoreOperation::Save(item) => self.cache.invalidate(&item.id),
                StoreOperation::Delete(id) => self.cache.invalidate(id),
            }
        }
        Ok(())
    }
}

#[async_trait]
impl<S: Store + BatchStore> BatchStore for CachingStore<S> {
    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        self.inner.save_batch(items).await?;
        for item in items {
            self.cache.invalidate(&item.id);
        }
        Ok(())
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        self.inner.load_batch(ids).await
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        self.inner.delete_batch(ids).await?;
        for id in ids {
            self.cache.invalidate(id);
        }
        Ok(())
    }
}

#[async_trait]
impl<S: Store + Maintainable> Maintainable for CachingStore<S> {
    async fn vacuum(&self) -> Result<(), StorageError> {
        self.inner.vacuum().await
    }

    async fn checkpoint(&self) -> Result<(), StorageError> {
        self.inner.checkpoint().await
    }

    async fn sync(&self) -> Result<(), StorageError> {
        self.inner.sync().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::memory::MemoryStorage;

    fn make_store() -> CachingStore<MemoryStorage> {
        CachingStore::new(MemoryStorage::new("test"), CacheConfig::default())
    }

    #[tokio::test]
    async fn test_load_populates_and_hits_cache() {
        let store = make_store();
        store
            .save("id1", b"data", &serde_json::json!({"status": "pending"}))
            .await
            .unwrap();
        let _ = store.load("id1").await.unwrap();
        assert_eq!(store.cache().hits(), 0);
        let _ = store.load("id1").await.unwrap();
        assert_eq!(store.cache().hits(), 1);
    }

    #[tokio::test]
    async fn test_update_status_invalidates_cache() {
        let store = make_store();
        store
            .save("id1", b"data", &serde_json::json!({"status": "pending"}))
            .await
            .unwrap();
        let _ = store.load("id1").await.unwrap();
        store.update_status("id1", "running").await.unwrap();
        let (_, meta) = store.load("id1").await.unwrap().unwrap();
        assert_eq!(meta["status"], "running");
    }

    #[tokio::test]
    async fn test_apply_batch_invalidates_cache() {
        let store = make_store();
        store
            .save("id1", b"old", &serde_json::json!({"v": 1}))
            .await
            .unwrap();
        let _ = store.load("id1").await.unwrap();
        store
            .apply_batch(&[StoreOperation::Save(BatchItem::new(
                "id1",
                b"new".to_vec(),
                serde_json::json!({"v": 2}),
            ))])
            .await
            .unwrap();
        let (data, _) = store.load("id1").await.unwrap().unwrap();
        assert_eq!(data, b"new");
    }

    #[tokio::test]
    async fn test_delete_invalidates_cache() {
        let store = make_store();
        store
            .save("id1", b"data", &serde_json::json!({}))
            .await
            .unwrap();
        let _ = store.load("id1").await.unwrap();
        store.delete("id1").await.unwrap();
        assert!(store.load("id1").await.unwrap().is_none());
    }
}

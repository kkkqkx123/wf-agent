use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::domain::store::{BatchItem, MetadataFilter, Store};
use crate::error::StorageError;

#[derive(Debug, Clone)]
struct StoredRecord {
    data: Vec<u8>,
    metadata: Value,
    created_at: i64,
}

#[derive(Debug)]
struct InnerStore {
    records: HashMap<String, StoredRecord>,
}

impl InnerStore {
    fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryStorage {
    inner: Arc<RwLock<InnerStore>>,
    #[allow(dead_code)]
    name: String,
}

impl MemoryStorage {
    pub fn new(name: &str) -> Self {
        Self {
            inner: Arc::new(RwLock::new(InnerStore::new())),
            name: name.to_string(),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new("default")
    }
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn apply_filter(
    records: &HashMap<String, StoredRecord>,
    filter: Option<&MetadataFilter>,
) -> Vec<(String, Value)> {
    let mut results: Vec<(String, Value)> = records
        .iter()
        .filter(|(_, rec)| {
            if let Some(f) = filter {
                if let Some(ref entity_type) = f.entity_type {
                    let rec_type = rec
                        .metadata
                        .get("entityType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if rec_type != entity_type {
                        return false;
                    }
                }
                if let Some(ref status) = f.status {
                    let rec_status = rec
                        .metadata
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if rec_status != status {
                        return false;
                    }
                }
            }
            true
        })
        .map(|(id, rec)| (id.clone(), rec.metadata.clone()))
        .collect();

    let offset = filter.and_then(|f| f.offset).unwrap_or(0) as usize;
    let limit = filter.and_then(|f| f.limit).unwrap_or(u64::MAX) as usize;

    results = results.into_iter().skip(offset).take(limit).collect();
    results
}

#[async_trait]
impl Store for MemoryStorage {
    async fn save(
        &self,
        id: &str,
        data: &[u8],
        metadata: &Value,
    ) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        let now = current_timestamp();
        let created_at = store.records.get(id).map(|r| r.created_at).unwrap_or(now);

        store.records.insert(
            id.to_string(),
            StoredRecord {
                data: data.to_vec(),
                metadata: metadata.clone(),
                created_at,
            },
        );
        Ok(())
    }

    async fn load(
        &self,
        id: &str,
    ) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        let store = self.inner.read().await;
        Ok(store.records.get(id).map(|rec| {
            (rec.data.clone(), rec.metadata.clone())
        }))
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        store.records.remove(id);
        Ok(())
    }

    async fn list(
        &self,
        filter: Option<&MetadataFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        let store = self.inner.read().await;
        Ok(apply_filter(&store.records, filter))
    }

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        let store = self.inner.read().await;
        Ok(store.records.contains_key(id))
    }

    async fn clear(&self) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        store.records.clear();
        Ok(())
    }
}

#[async_trait]
impl crate::domain::store::BatchStore for MemoryStorage {
    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        let now = current_timestamp();
        for item in items {
            store.records.insert(
                item.id.clone(),
                StoredRecord {
                    data: item.data.clone(),
                    metadata: item.metadata.clone(),
                    created_at: now,
                },
            );
        }
        Ok(())
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        for id in ids {
            store.records.remove(id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::store::BatchStore;

    #[tokio::test]
    async fn test_save_load_roundtrip() {
        let store = MemoryStorage::new("test");
        store
            .save("id1", b"hello", &serde_json::json!({"type": "test"}))
            .await
            .unwrap();
        let (data, meta) = store.load("id1").await.unwrap().unwrap();
        assert_eq!(data, b"hello");
        assert_eq!(meta, serde_json::json!({"type": "test"}));
    }

    #[tokio::test]
    async fn test_delete() {
        let store = MemoryStorage::new("test");
        store
            .save("id1", b"data", &serde_json::json!({}))
            .await
            .unwrap();
        assert!(store.exists("id1").await.unwrap());
        store.delete("id1").await.unwrap();
        assert!(!store.exists("id1").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_with_filter() {
        let store = MemoryStorage::new("test");
        store
            .save("id1", b"data1", &serde_json::json!({"entityType": "A", "status": "active"}))
            .await
            .unwrap();
        store
            .save("id2", b"data2", &serde_json::json!({"entityType": "B", "status": "inactive"}))
            .await
            .unwrap();
        store
            .save("id3", b"data3", &serde_json::json!({"entityType": "A", "status": "inactive"}))
            .await
            .unwrap();

        let filter = MetadataFilter {
            entity_type: Some("A".into()),
            status: None,
            offset: None,
            limit: None,
        };
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);

        let filter = MetadataFilter {
            entity_type: None,
            status: Some("inactive".into()),
            offset: None,
            limit: None,
        };
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let store = MemoryStorage::new("test");
        let items: Vec<BatchItem> = (0..100)
            .map(|i| {
                BatchItem::new(
                    format!("id_{}", i),
                    vec![i as u8; 100],
                    serde_json::json!({"index": i}),
                )
            })
            .collect();
        store.save_batch(&items).await.unwrap();
        assert_eq!(store.list(None).await.unwrap().len(), 100);

        let ids: Vec<String> = (0..50).map(|i| format!("id_{}", i)).collect();
        store.delete_batch(&ids).await.unwrap();
        assert_eq!(store.list(None).await.unwrap().len(), 50);
    }

    #[tokio::test]
    async fn test_close_does_not_clear_data() {
        let store = MemoryStorage::new("test");
        store
            .save("id1", b"data", &serde_json::json!({}))
            .await
            .unwrap();
        // MemoryStorage has no close() — data persists for the lifetime of the struct
        assert!(store.exists("id1").await.unwrap());
    }
}

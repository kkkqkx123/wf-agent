use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::domain::store::{BatchItem, FilterOp, QueryFilter, Store};
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
}

impl MemoryStorage {
    pub fn new(_name: &str) -> Self {
        Self {
            inner: Arc::new(RwLock::new(InnerStore::new())),
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

fn matches_meta_str(metadata: &Value, key: &str, expected: &str) -> bool {
    metadata
        .get(key)
        .and_then(|v| v.as_str())
        .map(|v| v == expected)
        .unwrap_or(false)
}

fn matches_meta_lt(metadata: &Value, key: &str, value: i64) -> bool {
    metadata
        .get(key)
        .and_then(|v| v.as_i64())
        .map(|v| v < value)
        .unwrap_or(false)
}

fn matches_meta_gt(metadata: &Value, key: &str, value: i64) -> bool {
    metadata
        .get(key)
        .and_then(|v| v.as_i64())
        .map(|v| v > value)
        .unwrap_or(false)
}

fn matches_meta_prefix(metadata: &Value, key: &str, prefix: &str) -> bool {
    metadata
        .get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.starts_with(prefix))
        .unwrap_or(false)
}

fn meta_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    if let (Some(x), Some(y)) = (a.as_i64(), b.as_i64()) {
        return x.cmp(&y);
    }
    if let (Some(x), Some(y)) = (a.as_f64(), b.as_f64()) {
        return x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
    }
    a.as_str()
        .unwrap_or_default()
        .cmp(b.as_str().unwrap_or_default())
}

fn matches_record(metadata: &Value, id: &str, op: &FilterOp) -> bool {
    match op {
        FilterOp::Eq(key, value) => matches_meta_str(metadata, key, value),
        FilterOp::IdPrefix(prefix) => id.starts_with(prefix),
        FilterOp::Prefix(key, prefix) => matches_meta_prefix(metadata, key, prefix),
        FilterOp::Lt(key, value) => matches_meta_lt(metadata, key, *value),
        FilterOp::Gt(key, value) => matches_meta_gt(metadata, key, *value),
        FilterOp::Between(key, start, end) => metadata
            .get(key)
            .and_then(|v| v.as_i64())
            .map(|ts| ts >= *start && ts <= *end)
            .unwrap_or(false),
        FilterOp::In(key, values) => matches_meta_in(metadata, key, values),
        FilterOp::OrderBy(_, _) | FilterOp::Offset(_) | FilterOp::Limit(_) => true,
    }
}

fn matches_meta_in(metadata: &Value, key: &str, values: &[String]) -> bool {
    metadata
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| values.iter().any(|v| v == s))
        .unwrap_or(false)
}

fn apply_filter(
    records: &HashMap<String, StoredRecord>,
    filter: Option<&QueryFilter>,
) -> Vec<(String, Value)> {
    let mut results: Vec<(String, Value)> = records
        .iter()
        .filter(|(id, rec)| {
            if let Some(f) = filter {
                for op in &f.ops {
                    if !matches_record(&rec.metadata, id, op) {
                        return false;
                    }
                }
            }
            true
        })
        .map(|(id, rec)| (id.clone(), rec.metadata.clone()))
        .collect();

    if let Some(f) = filter {
        for op in &f.ops {
            if let FilterOp::OrderBy(key, descending) = op {
                results.sort_by(|a, b| {
                    let cmp = meta_cmp(
                        a.1.get(key).unwrap_or(&Value::Null),
                        b.1.get(key).unwrap_or(&Value::Null),
                    );
                    if *descending {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }
        }

        let mut offset = 0usize;
        let mut limit = usize::MAX;
        for op in &f.ops {
            match op {
                FilterOp::Offset(o) => offset = *o as usize,
                FilterOp::Limit(l) => limit = *l as usize,
                _ => {}
            }
        }
        results = results.into_iter().skip(offset).take(limit).collect();
    }

    results
}

#[async_trait]
impl Store for MemoryStorage {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
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

    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        let store = self.inner.read().await;
        Ok(store
            .records
            .get(id)
            .map(|rec| (rec.data.clone(), rec.metadata.clone())))
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        store.records.remove(id);
        Ok(())
    }

    async fn list(
        &self,
        filter: Option<&QueryFilter>,
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

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        let store = self.inner.read().await;
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(rec) = store.records.get(id) {
                results.push((id.clone(), rec.data.clone(), rec.metadata.clone()));
            }
        }
        Ok(results)
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        for id in ids {
            store.records.remove(id);
        }
        Ok(())
    }
}

use crate::domain::store::Maintainable;

impl Maintainable for MemoryStorage {}

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
            .save(
                "id1",
                b"data1",
                &serde_json::json!({"entityType": "A", "status": "active"}),
            )
            .await
            .unwrap();
        store
            .save(
                "id2",
                b"data2",
                &serde_json::json!({"entityType": "B", "status": "inactive"}),
            )
            .await
            .unwrap();
        store
            .save(
                "id3",
                b"data3",
                &serde_json::json!({"entityType": "A", "status": "inactive"}),
            )
            .await
            .unwrap();

        let filter = QueryFilter::new().with_entity_type("A");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);

        let filter = QueryFilter::new().with_status("inactive");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_list_with_pushdown_ops() {
        let store = MemoryStorage::new("test");
        for i in 0..5 {
            store
                .save(
                    &format!("wf-{}:v{}", i, 1),
                    b"data",
                    &serde_json::json!({"entityType": "workflow", "timestamp": 1000 + i}),
                )
                .await
                .unwrap();
            store
                .save(
                    &format!("other-{}", i),
                    b"data",
                    &serde_json::json!({"entityType": "other", "timestamp": 2000 + i}),
                )
                .await
                .unwrap();
        }

        let filter = QueryFilter::new().with_id_prefix("wf-");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 5);

        let filter = QueryFilter::new()
            .with_field("entityType", "workflow")
            .with_order_by("timestamp", true)
            .with_limit(2);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1["timestamp"], 1004);
        assert_eq!(results[1].1["timestamp"], 1003);

        let filter = QueryFilter::new().with_field_lt("timestamp", 1003);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 3);
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

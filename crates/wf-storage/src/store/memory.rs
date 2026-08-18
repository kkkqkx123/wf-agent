use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::domain::store::{BatchItem, FilterOp, QueryFilter, Store, StoreOperation};
use crate::error::StorageError;

#[derive(Debug, Clone)]
struct StoredRecord {
    data: Vec<u8>,
    metadata: Value,
    hash: String,
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

    /// Test support: flip one byte of a stored record's payload without
    /// updating its integrity hash, simulating on-disk corruption. Returns
    /// whether the byte was flipped (record exists and offset in range).
    #[doc(hidden)]
    pub async fn corrupt_payload(&self, id: &str, offset: usize, value: u8) -> bool {
        let mut store = self.inner.write().await;
        if let Some(rec) = store.records.get_mut(id) {
            if offset < rec.data.len() {
                rec.data[offset] = value;
                return true;
            }
        }
        false
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

/// Text representation of a scalar metadata value, mirroring PostgreSQL's
/// `metadata->>'key'` operator: strings as-is, numbers in canonical decimal
/// form, booleans as 'true'/'false'. Null and structured values are excluded.
fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Numeric value truncated like PostgreSQL's `::bigint` cast, so numeric
/// predicates only ever see numbers and match non-numeric values never.
fn value_numeric_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        _ => None,
    }
}

fn matches_meta_str(metadata: &Value, key: &str, expected: &str) -> bool {
    metadata
        .get(key)
        .and_then(value_text)
        .map(|v| v == expected)
        .unwrap_or(false)
}

fn matches_meta_lt(metadata: &Value, key: &str, value: i64) -> bool {
    metadata
        .get(key)
        .and_then(value_numeric_int)
        .map(|v| v < value)
        .unwrap_or(false)
}

fn matches_meta_gt(metadata: &Value, key: &str, value: i64) -> bool {
    metadata
        .get(key)
        .and_then(value_numeric_int)
        .map(|v| v > value)
        .unwrap_or(false)
}

fn matches_meta_prefix(metadata: &Value, key: &str, prefix: &str) -> bool {
    metadata
        .get(key)
        .and_then(value_text)
        .map(|v| v.starts_with(prefix))
        .unwrap_or(false)
}

fn matches_meta_in(metadata: &Value, key: &str, values: &[String]) -> bool {
    metadata
        .get(key)
        .and_then(value_text)
        .map(|s| values.iter().any(|v| v == &s))
        .unwrap_or(false)
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
            .and_then(value_numeric_int)
            .map(|ts| ts >= *start && ts <= *end)
            .unwrap_or(false),
        FilterOp::In(key, values) => matches_meta_in(metadata, key, values),
        FilterOp::OrderBy(_, _) | FilterOp::Offset(_) | FilterOp::Limit(_) => true,
    }
}

/// Whether the metadata value is a JSON number. Numeric values sort before
/// everything else, matching PostgreSQL's `NULLS LAST` ordering where
/// non-numeric values and missing keys always come last.
fn is_numeric_value(value: &Value) -> bool {
    matches!(value, Value::Number(_))
}

fn meta_numeric_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    let a = a.as_f64().unwrap_or(f64::NEG_INFINITY);
    let b = b.as_f64().unwrap_or(f64::NEG_INFINITY);
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
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
                let descending = *descending;
                results.sort_by(|a, b| {
                    let va = a.1.get(key).unwrap_or(&Value::Null);
                    let vb = b.1.get(key).unwrap_or(&Value::Null);
                    match (is_numeric_value(va), is_numeric_value(vb)) {
                        // Numeric values always sort before non-numeric ones,
                        // matching PostgreSQL's `NULLS LAST` in both directions.
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        (true, true) => {
                            let cmp = meta_numeric_cmp(va, vb);
                            if descending {
                                cmp.reverse()
                            } else {
                                cmp
                            }
                        }
                        (false, false) => {
                            let cmp = value_text(va)
                                .unwrap_or_default()
                                .cmp(&value_text(vb).unwrap_or_default());
                            if descending {
                                cmp.reverse()
                            } else {
                                cmp
                            }
                        }
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
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        if let Some(rec) = store.records.get_mut(id) {
            if let Some(obj) = rec.metadata.as_object_mut() {
                obj.insert("status".to_string(), Value::String(status.to_string()));
            }
        }
        Ok(())
    }

    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        let now = current_timestamp();
        let created_at = store.records.get(id).map(|r| r.created_at).unwrap_or(now);

        store.records.insert(
            id.to_string(),
            StoredRecord {
                data: data.to_vec(),
                metadata: metadata.clone(),
                hash: crate::util::hash::compute_hash(data),
                created_at,
            },
        );
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError> {
        let store = self.inner.read().await;
        match store.records.get(id) {
            Some(rec) => {
                crate::util::hash::verify_integrity(id, &rec.data, &rec.hash)?;
                Ok(Some((rec.data.clone(), rec.metadata.clone())))
            }
            None => Ok(None),
        }
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

    async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        let store = self.inner.read().await;
        Ok(apply_filter(&store.records, filter).len() as u64)
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

    async fn apply_batch(&self, operations: &[StoreOperation]) -> Result<(), StorageError> {
        if operations.is_empty() {
            return Ok(());
        }
        // Atomicity under the single write lock: every operation is planned
        // (created_at preserved for overwritten records) before any mutation,
        // and the plan itself is infallible, so a batch either applies fully
        // or not at all — mirroring the transaction semantics of SQLite and
        // PostgreSQL backends (checkpoint cleanup watermark).
        let mut store = self.inner.write().await;
        let now = current_timestamp();
        let mut plan: Vec<(String, Option<StoredRecord>)> = Vec::with_capacity(operations.len());
        for operation in operations {
            match operation {
                StoreOperation::Save(item) => {
                    let created_at = store
                        .records
                        .get(&item.id)
                        .map(|r| r.created_at)
                        .unwrap_or(now);
                    plan.push((
                        item.id.clone(),
                        Some(StoredRecord {
                            data: item.data.clone(),
                            metadata: item.metadata.clone(),
                            hash: crate::util::hash::compute_hash(&item.data),
                            created_at,
                        }),
                    ));
                }
                StoreOperation::Delete(id) => {
                    plan.push((id.clone(), None));
                }
            }
        }
        for (id, state) in plan {
            match state {
                Some(record) => {
                    store.records.insert(id, record);
                }
                None => {
                    store.records.remove(&id);
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl crate::domain::store::BatchStore for MemoryStorage {
    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        let now = current_timestamp();
        for item in items {
            let created_at = store
                .records
                .get(&item.id)
                .map(|r| r.created_at)
                .unwrap_or(now);
            store.records.insert(
                item.id.clone(),
                StoredRecord {
                    data: item.data.clone(),
                    metadata: item.metadata.clone(),
                    hash: crate::util::hash::compute_hash(&item.data),
                    created_at,
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
                crate::util::hash::verify_integrity(id, &rec.data, &rec.hash)?;
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
    async fn test_filter_semantics_aligned() {
        let store = MemoryStorage::new("test");
        store
            .save(
                "n1",
                b"data",
                &serde_json::json!({"entityType": "wf", "timestamp": 1000, "flag": true}),
            )
            .await
            .unwrap();
        store
            .save(
                "s1",
                b"data",
                &serde_json::json!({"entityType": "wf", "timestamp": "abc", "flag": false}),
            )
            .await
            .unwrap();
        store
            .save(
                "s2",
                b"data",
                &serde_json::json!({"entityType": "wf", "timestamp": "500"}),
            )
            .await
            .unwrap();

        // Eq matches the text representation of numbers and booleans.
        let filter = QueryFilter::new().with_field("timestamp", "1000");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "n1");

        let filter = QueryFilter::new().with_field("flag", "true");
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "n1");

        // Numeric variants of the same number do not match text equality.
        let filter = QueryFilter::new().with_field("timestamp", "1e3");
        assert!(store.list(Some(&filter)).await.unwrap().is_empty());

        // Numeric predicates only match JSON numbers.
        let filter = QueryFilter::new().with_field_lt("timestamp", 1000);
        assert!(store.list(Some(&filter)).await.unwrap().is_empty());

        // OrderBy puts numeric values first in both directions.
        let filter = QueryFilter::new().with_order_by("timestamp", true);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results[0].0, "n1");
        assert_eq!(results.len(), 3);

        let filter = QueryFilter::new().with_order_by("timestamp", false);
        let results = store.list(Some(&filter)).await.unwrap();
        assert_eq!(results[0].0, "n1");
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

    #[tokio::test]
    async fn test_apply_batch_mixed_ops_atomic() {
        let store = MemoryStorage::new("test");
        for i in 0..5 {
            store
                .save(
                    &format!("cp-{}", i),
                    b"data",
                    &serde_json::json!({"entityType": "checkpoint", "index": i}),
                )
                .await
                .unwrap();
        }

        let operations = vec![
            StoreOperation::Delete("cp-0".to_string()),
            StoreOperation::Delete("cp-1".to_string()),
            StoreOperation::Save(BatchItem::new(
                "__watermark__:exec-1",
                Vec::new(),
                serde_json::json!({"cleanupWatermark": 1000}),
            )),
        ];
        store.apply_batch(&operations).await.unwrap();

        assert!(!store.exists("cp-0").await.unwrap());
        assert!(!store.exists("cp-1").await.unwrap());
        assert!(store.exists("cp-2").await.unwrap());
        let (_, meta) = store.load("__watermark__:exec-1").await.unwrap().unwrap();
        assert_eq!(meta["cleanupWatermark"], 1000);
    }

    #[tokio::test]
    async fn test_apply_batch_empty_is_noop() {
        let store = MemoryStorage::new("test");
        store.apply_batch(&[]).await.unwrap();
        assert!(store.list(None).await.unwrap().is_empty());
    }
}

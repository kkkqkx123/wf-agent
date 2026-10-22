use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::domain::store::{
    BatchItem, CompiledFilter, FilterCondition, QueryFilter, Store, StoreExt, StoreOperation,
};
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
    name: String,
    inner: Arc<RwLock<InnerStore>>,
    /// Metadata reads served so far. Test support only: lets a test pin that a
    /// hierarchy query costs a fixed number of reads instead of one per node.
    /// Kept beside the lock rather than inside it so a read stays a read.
    read_count: Arc<AtomicUsize>,
}

impl MemoryStorage {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            inner: Arc::new(RwLock::new(InnerStore::new())),
            read_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Test support: how many metadata reads this store has served. Paired with
    /// `reset_read_count` so a test can measure one query in isolation.
    #[doc(hidden)]
    pub fn read_count(&self) -> usize {
        self.read_count.load(Ordering::Relaxed)
    }

    /// Test support: zero the metadata read counter.
    #[doc(hidden)]
    pub fn reset_read_count(&self) {
        self.read_count.store(0, Ordering::Relaxed);
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

    /// Atomically apply operation groups across several memory stores.
    /// Groups must arrive in a globally deterministic order (the coordinator
    /// sorts by store identity) so concurrent cross-store batches acquire
    /// the write locks in the same order and cannot deadlock. No lock is
    /// held across an await while mutating, and planning is infallible, so
    /// either every group lands or none does.
    pub(crate) async fn apply_cross_store(
        groups: &[(&MemoryStorage, &[StoreOperation])],
    ) -> Result<(), StorageError> {
        let mut guards = Vec::with_capacity(groups.len());
        for (store, _) in groups {
            guards.push(store.inner.write().await);
        }
        let now = current_timestamp();
        for (mut guard, (_, operations)) in guards.into_iter().zip(groups.iter()) {
            for operation in *operations {
                match operation {
                    StoreOperation::Save(item) => {
                        let created_at = guard
                            .records
                            .get(&item.id)
                            .map(|r| r.created_at)
                            .unwrap_or(now);
                        guard.records.insert(
                            item.id.clone(),
                            StoredRecord {
                                data: item.data.clone(),
                                metadata: item.metadata.clone(),
                                hash: crate::util::hash::compute_hash(&item.data),
                                created_at,
                            },
                        );
                    }
                    StoreOperation::Delete(id) => {
                        guard.records.remove(id);
                    }
                }
            }
        }
        Ok(())
    }

    /// Clear several memory stores atomically. Stores must arrive in the
    /// same globally deterministic order as `apply_cross_store` so
    /// concurrent cross-store operations acquire the write locks in one
    /// order and cannot deadlock. Clearing is infallible, so either every
    /// store is emptied or none is.
    pub(crate) async fn clear_cross_store(stores: &[&MemoryStorage]) -> Result<(), StorageError> {
        let mut guards = Vec::with_capacity(stores.len());
        for store in stores {
            guards.push(store.inner.write().await);
        }
        for mut guard in guards {
            guard.records.clear();
        }
        Ok(())
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

/// Numeric value as float, mirroring PostgreSQL's `::float8` comparison so
/// integer and floating-point metadata stay comparable while non-numeric
/// values never match.
fn value_numeric(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
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
        .and_then(value_numeric)
        .map(|v| v < value as f64)
        .unwrap_or(false)
}

fn matches_meta_gt(metadata: &Value, key: &str, value: i64) -> bool {
    metadata
        .get(key)
        .and_then(value_numeric)
        .map(|v| v > value as f64)
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

fn matches_condition(metadata: &Value, id: &str, cond: &FilterCondition) -> bool {
    match cond {
        FilterCondition::Eq(key, value) => matches_meta_str(metadata, key, value),
        FilterCondition::Id(exact) => id == exact,
        FilterCondition::IdPrefix(prefix) => id.starts_with(prefix),
        FilterCondition::Prefix(key, prefix) => matches_meta_prefix(metadata, key, prefix),
        FilterCondition::Lt(key, value) => matches_meta_lt(metadata, key, *value),
        FilterCondition::Gt(key, value) => matches_meta_gt(metadata, key, *value),
        FilterCondition::Between(key, start, end) => metadata
            .get(key)
            .and_then(value_numeric)
            .map(|ts| ts >= *start as f64 && ts <= *end as f64)
            .unwrap_or(false),
        FilterCondition::In(key, values) => matches_meta_in(metadata, key, values),
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

/// Ids matching a compiled plan, in result order. Only ids are collected
/// here so `count` never clones payloads; callers project the columns they
/// need afterwards. The plan comes from `QueryFilter::compile`, so repeated
/// ordering and pagination resolve to the last occurrence exactly like the
/// SQL backends. An id tie-break follows the primary sort direction so ties
/// resolve to creation order on both newest-first and oldest-first queries.
fn matched_ids(
    records: &HashMap<String, StoredRecord>,
    plan: Option<&CompiledFilter>,
) -> Vec<String> {
    let mut ids: Vec<&String> = records
        .iter()
        .filter(|(id, rec)| {
            plan.map(|p| {
                p.conditions
                    .iter()
                    .all(|cond| matches_condition(&rec.metadata, id, cond))
            })
            .unwrap_or(true)
        })
        .map(|(id, _)| id)
        .collect();

    if let Some(p) = plan {
        if let Some((key, descending)) = p.order_by.clone() {
            ids.sort_by(|a, b| {
                let ra = &records[*a];
                let rb = &records[*b];
                let va = ra.metadata.get(&key).unwrap_or(&Value::Null);
                let vb = rb.metadata.get(&key).unwrap_or(&Value::Null);
                let primary = match (is_numeric_value(va), is_numeric_value(vb)) {
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
                };
                primary.then_with(|| if descending { b.cmp(a) } else { a.cmp(b) })
            });
        }
        let offset = p.offset.unwrap_or(0) as usize;
        let limit = p.limit.unwrap_or(u64::MAX) as usize;
        ids = ids.into_iter().skip(offset).take(limit).collect();
    }

    ids.into_iter().cloned().collect()
}

fn apply_filter(
    records: &HashMap<String, StoredRecord>,
    filter: Option<&QueryFilter>,
) -> Vec<(String, Value)> {
    let plan = filter.map(|f| f.compile());
    matched_ids(records, plan.as_ref())
        .into_iter()
        .filter_map(|id| records.get(&id).map(|rec| (id, rec.metadata.clone())))
        .collect()
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
        self.read_count.fetch_add(1, Ordering::Relaxed);
        let store = self.inner.read().await;
        Ok(apply_filter(&store.records, filter))
    }

    async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        // Counting reports total matches without cloning payloads and
        // without applying pagination, matching the SQL backends.
        let plan = filter.map(|f| f.compile());
        let mut plan = plan.unwrap_or_default();
        plan.order_by = None;
        plan.offset = None;
        plan.limit = None;
        let store = self.inner.read().await;
        Ok(matched_ids(&store.records, Some(&plan)).len() as u64)
    }

    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        let plan = filter.map(|f| f.compile());
        let store = self.inner.read().await;
        let ids = matched_ids(&store.records, plan.as_ref());
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(rec) = store.records.get(&id) {
                crate::util::hash::verify_integrity(&id, &rec.data, &rec.hash)?;
                results.push((rec.data.clone(), rec.metadata.clone()));
            }
        }
        Ok(results)
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
impl StoreExt for MemoryStorage {
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        let mut store = self.inner.write().await;
        if let Some(rec) = store.records.get_mut(id) {
            if let Some(obj) = rec.metadata.as_object_mut() {
                obj.insert("status".to_string(), Value::String(status.to_string()));
            }
        }
        Ok(())
    }

    async fn apply_batch(&self, operations: &[StoreOperation]) -> Result<(), StorageError> {
        if operations.is_empty() {
            return Ok(());
        }
        // Atomicity: the entire plan (clone + compute hash) is infallible, and
        // mutations execute under a single exclusive write lock. No intermediate
        // state is visible to other tasks, matching Sqlite/PG transaction
        // semantics (checkpoint cleanup watermark).
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
#[path = "memory/tests.rs"]
mod tests;

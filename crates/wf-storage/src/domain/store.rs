use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::StorageError;

/// A single query operation that backends can push down to their native query language.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    /// Metadata field equals a string value.
    Eq(String, String),
    /// Record id starts with a prefix.
    IdPrefix(String),
    /// Metadata field starts with a prefix.
    Prefix(String, String),
    /// Metadata field is numerically less than a value.
    Lt(String, i64),
    /// Metadata field is numerically greater than a value.
    Gt(String, i64),
    /// Metadata field is within an inclusive range.
    Between(String, i64, i64),
    /// Order results by metadata field; second value true = descending.
    OrderBy(String, bool),
    /// Skip N results.
    Offset(u64),
    /// Take at most N results.
    Limit(u64),
}

#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub ops: Vec<FilterOp>,
}

impl QueryFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_op(&mut self, op: FilterOp) {
        self.ops.push(op);
    }

    pub fn with_entity_type(mut self, entity_type: &str) -> Self {
        self.ops
            .push(FilterOp::Eq("entityType".into(), entity_type.into()));
        self
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.ops.push(FilterOp::Eq("status".into(), status.into()));
        self
    }

    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.ops.push(FilterOp::Eq(key.into(), value.into()));
        self
    }

    pub fn with_id_prefix(mut self, prefix: &str) -> Self {
        self.ops.push(FilterOp::IdPrefix(prefix.into()));
        self
    }

    pub fn with_field_prefix(mut self, key: &str, prefix: &str) -> Self {
        self.ops.push(FilterOp::Prefix(key.into(), prefix.into()));
        self
    }

    pub fn with_field_lt(mut self, key: &str, value: i64) -> Self {
        self.ops.push(FilterOp::Lt(key.into(), value));
        self
    }

    pub fn with_field_gt(mut self, key: &str, value: i64) -> Self {
        self.ops.push(FilterOp::Gt(key.into(), value));
        self
    }

    pub fn with_timestamp_range(mut self, start: i64, end: i64) -> Self {
        self.ops
            .push(FilterOp::Between("timestamp".into(), start, end));
        self
    }

    pub fn with_order_by(mut self, field: &str, descending: bool) -> Self {
        self.ops.push(FilterOp::OrderBy(field.into(), descending));
        self
    }

    pub fn with_offset(mut self, offset: u64) -> Self {
        self.ops.push(FilterOp::Offset(offset));
        self
    }

    pub fn with_limit(mut self, limit: u64) -> Self {
        self.ops.push(FilterOp::Limit(limit));
        self
    }
}

pub struct BatchItem {
    pub id: String,
    pub data: Vec<u8>,
    pub metadata: Value,
}

impl BatchItem {
    pub fn new(id: impl Into<String>, data: Vec<u8>, metadata: Value) -> Self {
        Self {
            id: id.into(),
            data,
            metadata,
        }
    }
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError>;
    async fn delete(&self, id: &str) -> Result<(), StorageError>;
    async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError>;
    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        let entries = self.list(filter).await?;
        let mut results = Vec::with_capacity(entries.len());
        for (id, _) in entries {
            if let Some((data, metadata)) = self.load(&id).await? {
                results.push((data, metadata));
            }
        }
        Ok(results)
    }
    async fn exists(&self, id: &str) -> Result<bool, StorageError>;

    async fn clear(&self) -> Result<(), StorageError>;

    /// Count records grouped by a string metadata field.
    /// Backends may override with an aggregate query (e.g. GROUP BY).
    async fn count_by_metadata_field(
        &self,
        field: &str,
    ) -> Result<HashMap<String, u64>, StorageError> {
        let entries = self.list(None).await?;
        let mut counts = HashMap::new();
        for (_, meta) in entries {
            let key = match meta.get(field) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Bool(b)) => b.to_string(),
                Some(Value::Number(n)) => n.to_string(),
                _ => continue,
            };
            *counts.entry(key).or_insert(0) += 1;
        }
        Ok(counts)
    }
}

#[async_trait]
pub trait BatchStore: Store {
    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        for item in items {
            self.save(&item.id, &item.data, &item.metadata).await?;
        }
        Ok(())
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some((data, metadata)) = self.load(id).await? {
                results.push((id.clone(), data, metadata));
            }
        }
        Ok(results)
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        for id in ids {
            self.delete(id).await?;
        }
        Ok(())
    }
}

#[async_trait]
pub trait Maintainable: Store {
    async fn vacuum(&self) -> Result<(), StorageError> {
        Ok(())
    }
    async fn checkpoint(&self) -> Result<(), StorageError> {
        Ok(())
    }
    /// Flush pending writes to durable storage.
    /// For SQLite with WAL mode, this runs a WAL checkpoint to ensure committed
    /// transactions are written to the main database file.
    /// For PostgreSQL this is a no-op (fsync on every commit is guaranteed).
    async fn sync(&self) -> Result<(), StorageError> {
        Ok(())
    }
}

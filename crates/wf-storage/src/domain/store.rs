use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub entity_type: Option<String>,
    pub status: Option<String>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub fields: HashMap<String, String>,
    pub timestamp_range: Option<(i64, i64)>,
}

impl QueryFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_entity_type(mut self, entity_type: &str) -> Self {
        self.entity_type = Some(entity_type.to_string());
        self
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.status = Some(status.to_string());
        self
    }

    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn with_limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_timestamp_range(mut self, start: i64, end: i64) -> Self {
        self.timestamp_range = Some((start, end));
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

use async_trait::async_trait;
use serde_json::Value;

use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct MetadataFilter {
    pub entity_type: Option<String>,
    pub status: Option<String>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
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
    async fn save(
        &self,
        id: &str,
        data: &[u8],
        metadata: &Value,
    ) -> Result<(), StorageError>;
    async fn load(
        &self,
        id: &str,
    ) -> Result<Option<(Vec<u8>, Value)>, StorageError>;
    async fn delete(&self, id: &str) -> Result<(), StorageError>;
    async fn list(
        &self,
        filter: Option<&MetadataFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError>;
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
}

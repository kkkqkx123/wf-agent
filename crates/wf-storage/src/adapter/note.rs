use crate::error::StorageError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoteEntity {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

pub trait NoteStorageAdapter: Send + Sync {
    async fn save(&self, entity: &NoteEntity) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Option<NoteEntity>, StorageError>;
    async fn delete(&self, id: &str) -> Result<bool, StorageError>;
    async fn list_all(&self) -> Result<Vec<NoteEntity>, StorageError>;
    async fn search(&self, query: &str) -> Result<Vec<NoteEntity>, StorageError>;
    async fn clear(&self) -> Result<(), StorageError>;
}

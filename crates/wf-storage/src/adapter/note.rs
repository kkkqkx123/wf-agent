use crate::error::StorageError;
use serde::{Deserialize, Serialize};
use std::future::Future;

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
    fn save<'a>(
        &'a self,
        entity: &'a NoteEntity,
    ) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;
    fn load<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Future<Output = Result<Option<NoteEntity>, StorageError>> + Send + 'a;
    fn delete<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Future<Output = Result<bool, StorageError>> + Send + 'a;
    fn list_all<'a>(
        &'a self,
    ) -> impl Future<Output = Result<Vec<NoteEntity>, StorageError>> + Send + 'a;
    fn search<'a>(
        &'a self,
        query: &'a str,
    ) -> impl Future<Output = Result<Vec<NoteEntity>, StorageError>> + Send + 'a;
    fn clear<'a>(&'a self) -> impl Future<Output = Result<(), StorageError>> + Send + 'a;
}

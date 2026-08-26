use serde_json::Value;

use crate::adapter::note::{NoteEntity, NoteStorageAdapter};
use crate::domain::entity::Entity;
use crate::domain::store::Store;
use crate::error::StorageError;
use crate::store::entity_store::EntityStore;

impl Entity for NoteEntity {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "note"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "title": self.title,
            "updatedAt": self.updated_at,
            "tags": self.tags,
        })
    }
}

/// Note storage backed by a regular [`Store`] backend, so notes persist on
/// the same durable backend as every other entity (Sqlite / PostgreSQL).
/// Search reuses `list_all` with an in-memory filter, matching the semantics
/// of the former `MemoryNoteStore`.
pub struct StoreBackedNoteStore<S> {
    entity_store: EntityStore<S, NoteEntity>,
}

impl<S: Store> StoreBackedNoteStore<S> {
    pub fn new(storage: S) -> Self {
        Self {
            entity_store: EntityStore::new(storage),
        }
    }

    pub fn inner(&self) -> &S {
        self.entity_store.inner()
    }
}

impl<S: Store> NoteStorageAdapter for StoreBackedNoteStore<S> {
    async fn save(&self, entity: &NoteEntity) -> Result<(), StorageError> {
        self.entity_store.save(entity).await
    }

    async fn load(&self, id: &str) -> Result<Option<NoteEntity>, StorageError> {
        self.entity_store.load(id).await
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        let existed = self.entity_store.exists(id).await?;
        self.entity_store.delete(id).await?;
        Ok(existed)
    }

    async fn list_all(&self) -> Result<Vec<NoteEntity>, StorageError> {
        self.entity_store.list(None).await
    }

    async fn search(&self, query: &str) -> Result<Vec<NoteEntity>, StorageError> {
        let query_lower = query.to_lowercase();
        Ok(self
            .entity_store
            .list(None)
            .await?
            .into_iter()
            .filter(|note| {
                note.title.to_lowercase().contains(&query_lower)
                    || note.content.to_lowercase().contains(&query_lower)
            })
            .collect())
    }

    async fn clear(&self) -> Result<(), StorageError> {
        self.entity_store.clear().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::memory::MemoryStorage;

    fn sample_note(id: &str) -> NoteEntity {
        NoteEntity {
            id: id.to_string(),
            title: "Test Note".to_string(),
            content: "Hello World".to_string(),
            created_at: 0,
            updated_at: 0,
            tags: Some(vec!["test".to_string()]),
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_note_crud() {
        let store = StoreBackedNoteStore::new(MemoryStorage::new("note"));
        let note = sample_note("test-1");

        store.save(&note).await.unwrap();
        let loaded = store.load("test-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().title, "Test Note");

        let results = store.search("hello").await.unwrap();
        assert_eq!(results.len(), 1);

        let deleted = store.delete("test-1").await.unwrap();
        assert!(deleted);

        let loaded = store.load("test-1").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_note_delete_missing_returns_false() {
        let store = StoreBackedNoteStore::new(MemoryStorage::new("note"));
        assert!(!store.delete("missing").await.unwrap());
    }

    #[tokio::test]
    async fn test_note_metadata_contains_entity_type() {
        let store = StoreBackedNoteStore::new(MemoryStorage::new("note"));
        store.save(&sample_note("n1")).await.unwrap();
        let entries = store.inner().list(None).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1["entityType"], "note");
    }

    #[tokio::test]
    async fn test_note_clear() {
        let store = StoreBackedNoteStore::new(MemoryStorage::new("note"));
        store.save(&sample_note("n1")).await.unwrap();
        store.save(&sample_note("n2")).await.unwrap();
        assert_eq!(store.list_all().await.unwrap().len(), 2);

        store.clear().await.unwrap();
        assert!(store.list_all().await.unwrap().is_empty());
    }
}

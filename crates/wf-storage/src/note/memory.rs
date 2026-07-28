use crate::adapter::note::{NoteEntity, NoteStorageAdapter};
use crate::error::StorageError;
use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub struct MemoryNoteStore {
    notes: RwLock<HashMap<String, NoteEntity>>,
}

impl MemoryNoteStore {
    pub fn new() -> Self {
        Self {
            notes: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryNoteStore {
    fn default() -> Self {
        Self::new()
    }
}

fn read_notes(store: &MemoryNoteStore) -> Result<RwLockReadGuard<'_, HashMap<String, NoteEntity>>, StorageError> {
    store.notes.read().map_err(|_| StorageError::General {
        operation: "note_read".to_string(),
        message: "lock poisoned".to_string(),
        source: None,
    })
}

fn write_notes(store: &MemoryNoteStore) -> Result<RwLockWriteGuard<'_, HashMap<String, NoteEntity>>, StorageError> {
    store.notes.write().map_err(|_| StorageError::General {
        operation: "note_write".to_string(),
        message: "lock poisoned".to_string(),
        source: None,
    })
}

impl NoteStorageAdapter for MemoryNoteStore {
    async fn save(&self, entity: &NoteEntity) -> Result<(), StorageError> {
        let mut notes = write_notes(self)?;
        notes.insert(entity.id.clone(), entity.clone());
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<NoteEntity>, StorageError> {
        let notes = read_notes(self)?;
        Ok(notes.get(id).cloned())
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        let mut notes = write_notes(self)?;
        Ok(notes.remove(id).is_some())
    }

    async fn list_all(&self) -> Result<Vec<NoteEntity>, StorageError> {
        let notes = read_notes(self)?;
        Ok(notes.values().cloned().collect())
    }

    async fn search(&self, query: &str) -> Result<Vec<NoteEntity>, StorageError> {
        let query_lower = query.to_lowercase();
        let notes = read_notes(self)?;
        let results: Vec<NoteEntity> = notes
            .values()
            .filter(|note| {
                note.title.to_lowercase().contains(&query_lower)
                    || note.content.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();
        Ok(results)
    }

    async fn clear(&self) -> Result<(), StorageError> {
        let mut notes = write_notes(self)?;
        notes.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_note_crud() {
        let store = MemoryNoteStore::new();
        let note = NoteEntity {
            id: "test-1".to_string(),
            title: "Test Note".to_string(),
            content: "Hello World".to_string(),
            created_at: 0,
            updated_at: 0,
            tags: Some(vec!["test".to_string()]),
            metadata: None,
        };

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
}

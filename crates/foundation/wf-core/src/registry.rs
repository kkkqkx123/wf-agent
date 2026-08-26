use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::Serialize;

#[derive(thiserror::Error, Debug)]
pub enum RegistryError {
    #[error("key already exists: {key}")]
    AlreadyExists { key: String },
    #[error("key not found: {key}")]
    NotFound { key: String },
    #[error("validation failed: {message}")]
    ValidationError { message: String },
    #[error("batch operation partially failed: {succeeded}/{total} succeeded")]
    BatchPartialFailure { succeeded: usize, total: usize },
    #[error("serialization error: {message}")]
    SerializationError { message: String },
    #[error("storage error: {message}")]
    StorageError { message: String },
    #[error("reference exists: {message}")]
    ReferenceExists { message: String },
}

pub type RegistryResult<T> = Result<T, RegistryError>;

/// Validator function invoked on every `register()` call.
type Validator<T> = dyn Fn(&str, &T) -> RegistryResult<()> + Send + Sync;

pub trait Registry<T: Send + Sync>: Send + Sync {
    fn get(&self, key: &str) -> Option<Arc<T>>;
    fn has(&self, key: &str) -> bool;
    fn list(&self) -> Vec<String>;
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
}

pub trait MutableRegistry<T: Send + Sync>: Registry<T> {
    /// Register an item. Fails with `AlreadyExists` if key already exists.
    fn register(&self, key: String, item: Arc<T>) -> RegistryResult<()>;
    /// Register an item, replacing any existing item with the same key.
    /// Never fails; existing items are silently replaced.
    fn register_or_replace(&self, key: String, item: Arc<T>) -> Option<Arc<T>>;
    fn unregister(&self, key: &str) -> Option<Arc<T>>;
    fn clear(&self);

    /// Set a validator function that is called on every `register()` call.
    /// The validator receives the key and a reference to the item, and
    /// returns `Ok(())` if the item is valid, or `Err(ValidationError)`.
    ///
    /// Default implementation is a no-op.
    fn set_validator(&self, _validator: Arc<Validator<T>>) {}
}

pub trait BatchRegistry<T: Send + Sync>: MutableRegistry<T> {
    fn register_batch(&self, items: Vec<(String, Arc<T>)>) -> RegistryResult<()>;
    fn unregister_batch(&self, keys: &[String]) -> Vec<Option<Arc<T>>>;
}

// ── Searchable ──

/// Generic search capability for registries.
///
/// Implementors define how items are matched — by key prefix, by tag,
/// or by custom field extraction.
pub trait Searchable {
    type Item;
    /// Return keys of items matching `query`.
    fn search(&self, query: &str) -> Vec<String>;
    /// Return keys of items tagged with `tag`.
    fn list_by_tag(&self, tag: &str) -> Vec<String>;
}

// ── Exportable ──

/// Export/import registry items as JSON values.
pub trait Exportable {
    type Item: Serialize;
    /// Export all items as `(key, json_value)` pairs.
    fn export_all(&self) -> RegistryResult<Vec<(String, serde_json::Value)>>;
    /// Import items from `(key, json_value)` pairs.
    ///
    /// When `skip_if_exists` is true, existing keys are silently skipped;
    /// otherwise a duplicate key returns `AlreadyExists`.
    fn import(
        &self,
        entries: Vec<(String, serde_json::Value)>,
        skip_if_exists: bool,
    ) -> RegistryResult<()>;
}

// ── ReferenceCheckable ──

/// A reference from one registry item to another.
#[derive(Debug, Clone)]
pub struct Ref {
    /// The id of the item that holds the reference.
    pub source_id: String,
    /// The field / role name of the reference.
    pub field: String,
}

/// Check whether a key is referenced by other items before deletion.
pub trait ReferenceCheckable {
    /// Return all references pointing to `id`.
    fn check_references(&self, id: &str) -> Vec<Ref>;
    /// Return `Ok` iff `id` has no references; otherwise return
    /// `Err(ReferenceExists)`.
    fn can_delete(&self, id: &str) -> RegistryResult<()> {
        let refs = self.check_references(id);
        if refs.is_empty() {
            Ok(())
        } else {
            let msg = refs
                .into_iter()
                .map(|r| format!("{}#{}", r.source_id, r.field))
                .collect::<Vec<_>>()
                .join(", ");
            Err(RegistryError::ReferenceExists { message: msg })
        }
    }
}

// ── PersistableStorage ──

/// A simple key-value storage backend for registry persistence.
///
/// This is intentionally minimal and sync-agnostic. Higher-level adapters
/// (`wf-storage::Store`) offer richer query semantics.
#[async_trait]
pub trait PersistableStorage: Send + Sync {
    async fn save(&self, key: &str, data: &[u8]) -> RegistryResult<()>;
    async fn load(&self, key: &str) -> RegistryResult<Option<Vec<u8>>>;
    async fn delete(&self, key: &str) -> RegistryResult<()>;
    async fn list(&self, prefix: &str) -> RegistryResult<Vec<String>>;
}

// ── PersistableRegistry ──

/// A registry that can persist its entire state to and restore from a
/// [`PersistableStorage`] backend.
#[async_trait]
pub trait PersistableRegistry<T: Send + Sync>: Registry<T> {
    /// Persist all current items to `storage`.
    async fn persist(&self, storage: &dyn PersistableStorage) -> RegistryResult<()>;
    /// Restore all items from `storage`, clearing any in-memory state first.
    async fn restore(&self, storage: &dyn PersistableStorage) -> RegistryResult<()>;
}

// ── ConcurrentRegistry ──

pub struct ConcurrentRegistry<T: Send + Sync> {
    items: DashMap<String, Arc<T>>,
    validator: Mutex<Option<Arc<Validator<T>>>>,
}

impl<T: Send + Sync> ConcurrentRegistry<T> {
    pub fn new() -> Self {
        Self {
            items: DashMap::new(),
            validator: Mutex::new(None),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: DashMap::with_capacity(capacity),
            validator: Mutex::new(None),
        }
    }
}

impl<T: Send + Sync> Default for ConcurrentRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Registry<T> for ConcurrentRegistry<T> {
    fn get(&self, key: &str) -> Option<Arc<T>> {
        self.items.get(key).map(|entry| entry.value().clone())
    }

    fn has(&self, key: &str) -> bool {
        self.items.contains_key(key)
    }

    fn list(&self) -> Vec<String> {
        self.items.iter().map(|entry| entry.key().clone()).collect()
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

impl<T: Send + Sync> MutableRegistry<T> for ConcurrentRegistry<T> {
    fn register(&self, key: String, item: Arc<T>) -> RegistryResult<()> {
        if self.items.contains_key(&key) {
            return Err(RegistryError::AlreadyExists { key });
        }
        // Run validator if one is set
        if let Some(ref validator) = *self.validator.lock().unwrap() {
            validator(&key, &item)?;
        }
        self.items.insert(key, item);
        Ok(())
    }

    fn register_or_replace(&self, key: String, item: Arc<T>) -> Option<Arc<T>> {
        self.items.insert(key, item)
    }

    fn set_validator(&self, validator: Arc<dyn Fn(&str, &T) -> RegistryResult<()> + Send + Sync>) {
        *self.validator.lock().unwrap() = Some(validator);
    }

    fn unregister(&self, key: &str) -> Option<Arc<T>> {
        self.items.remove(key).map(|(_, v)| v)
    }

    fn clear(&self) {
        self.items.clear();
    }
}

impl<T: Send + Sync> BatchRegistry<T> for ConcurrentRegistry<T> {
    fn register_batch(&self, items: Vec<(String, Arc<T>)>) -> RegistryResult<()> {
        let total = items.len();
        let mut succeeded = 0;
        for (key, item) in items {
            if self.items.contains_key(&key) {
                continue;
            }
            self.items.insert(key, item);
            succeeded += 1;
        }
        if succeeded < total {
            return Err(RegistryError::BatchPartialFailure { succeeded, total });
        }
        Ok(())
    }

    fn unregister_batch(&self, keys: &[String]) -> Vec<Option<Arc<T>>> {
        keys.iter().map(|k| self.unregister(k)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("value_a".to_string()))
            .unwrap();
        assert_eq!(reg.get("a"), Some(Arc::new("value_a".to_string())));
        assert!(reg.has("a"));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_register_duplicate() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("v1".to_string()))
            .unwrap();
        let result = reg.register("a".to_string(), Arc::new("v2".to_string()));
        assert!(matches!(result, Err(RegistryError::AlreadyExists { .. })));
    }

    #[test]
    fn test_unregister() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("value_a".to_string()))
            .unwrap();
        let removed = reg.unregister("a");
        assert_eq!(removed, Some(Arc::new("value_a".to_string())));
        assert!(!reg.has("a"));
        assert!(reg.is_empty());
    }

    #[test]
    fn test_unregister_missing() {
        let reg = ConcurrentRegistry::<String>::new();
        assert_eq!(reg.unregister("nonexistent"), None);
    }

    #[test]
    fn test_list() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("b".to_string(), Arc::new("v_b".to_string()))
            .unwrap();
        reg.register("a".to_string(), Arc::new("v_a".to_string()))
            .unwrap();
        let mut keys = reg.list();
        keys.sort();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    fn test_clear() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("v_a".to_string()))
            .unwrap();
        reg.register("b".to_string(), Arc::new("v_b".to_string()))
            .unwrap();
        reg.clear();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_register_batch() {
        let reg = ConcurrentRegistry::<String>::new();
        let items = vec![
            ("a".to_string(), Arc::new("v_a".to_string())),
            ("b".to_string(), Arc::new("v_b".to_string())),
        ];
        reg.register_batch(items).unwrap();
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn test_register_batch_partial_failure() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("v_a".to_string()))
            .unwrap();
        let items = vec![
            ("a".to_string(), Arc::new("v_a2".to_string())),
            ("b".to_string(), Arc::new("v_b".to_string())),
        ];
        let result = reg.register_batch(items);
        assert!(matches!(
            result,
            Err(RegistryError::BatchPartialFailure {
                succeeded: 1,
                total: 2
            })
        ));
    }

    #[test]
    fn test_unregister_batch() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("v_a".to_string()))
            .unwrap();
        reg.register("b".to_string(), Arc::new("v_b".to_string()))
            .unwrap();
        reg.register("c".to_string(), Arc::new("v_c".to_string()))
            .unwrap();
        let results = reg.unregister_batch(&["a".to_string(), "c".to_string()]);
        assert_eq!(results.len(), 2);
        assert!(results[0].is_some());
        assert!(results[1].is_some());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_with_capacity() {
        let reg = ConcurrentRegistry::<String>::with_capacity(100);
        assert!(reg.is_empty());
    }

    #[test]
    fn test_register_or_replace() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("v1".to_string()))
            .unwrap();
        assert_eq!(reg.get("a").unwrap().as_str(), "v1");

        let old = reg.register_or_replace("a".to_string(), Arc::new("v2".to_string()));
        assert!(old.is_some());
        assert_eq!(old.unwrap().as_str(), "v1");
        assert_eq!(reg.get("a").unwrap().as_str(), "v2");

        let old_none = reg.register_or_replace("b".to_string(), Arc::new("v3".to_string()));
        assert!(old_none.is_none());
        assert_eq!(reg.get("b").unwrap().as_str(), "v3");
    }
}

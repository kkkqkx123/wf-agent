use std::sync::Arc;

use dashmap::DashMap;

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
}

pub type RegistryResult<T> = Result<T, RegistryError>;

pub trait Registry<T: Send + Sync>: Send + Sync {
    fn get(&self, key: &str) -> Option<Arc<T>>;
    fn has(&self, key: &str) -> bool;
    fn list(&self) -> Vec<String>;
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
}

pub trait MutableRegistry<T: Send + Sync>: Registry<T> {
    fn register(&self, key: String, item: Arc<T>) -> RegistryResult<()>;
    fn unregister(&self, key: &str) -> Option<Arc<T>>;
    fn clear(&self);
}

pub trait BatchRegistry<T: Send + Sync>: MutableRegistry<T> {
    fn register_batch(&self, items: Vec<(String, Arc<T>)>) -> RegistryResult<()>;
    fn unregister_batch(&self, keys: &[String]) -> Vec<Option<Arc<T>>>;
}

pub struct ConcurrentRegistry<T: Send + Sync> {
    items: DashMap<String, Arc<T>>,
}

impl<T: Send + Sync> ConcurrentRegistry<T> {
    pub fn new() -> Self {
        Self {
            items: DashMap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: DashMap::with_capacity(capacity),
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
        self.items.insert(key, item);
        Ok(())
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
        reg.register("a".to_string(), Arc::new("value_a".to_string())).unwrap();
        assert_eq!(reg.get("a"), Some(Arc::new("value_a".to_string())));
        assert!(reg.has("a"));
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_register_duplicate() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("v1".to_string())).unwrap();
        let result = reg.register("a".to_string(), Arc::new("v2".to_string()));
        assert!(matches!(result, Err(RegistryError::AlreadyExists { .. })));
    }

    #[test]
    fn test_unregister() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("value_a".to_string())).unwrap();
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
        reg.register("b".to_string(), Arc::new("v_b".to_string())).unwrap();
        reg.register("a".to_string(), Arc::new("v_a".to_string())).unwrap();
        let mut keys = reg.list();
        keys.sort();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    fn test_clear() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("v_a".to_string())).unwrap();
        reg.register("b".to_string(), Arc::new("v_b".to_string())).unwrap();
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
        reg.register("a".to_string(), Arc::new("v_a".to_string())).unwrap();
        let items = vec![
            ("a".to_string(), Arc::new("v_a2".to_string())),
            ("b".to_string(), Arc::new("v_b".to_string())),
        ];
        let result = reg.register_batch(items);
        assert!(matches!(result, Err(RegistryError::BatchPartialFailure { succeeded: 1, total: 2 })));
    }

    #[test]
    fn test_unregister_batch() {
        let reg = ConcurrentRegistry::<String>::new();
        reg.register("a".to_string(), Arc::new("v_a".to_string())).unwrap();
        reg.register("b".to_string(), Arc::new("v_b".to_string())).unwrap();
        reg.register("c".to_string(), Arc::new("v_c".to_string())).unwrap();
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
}

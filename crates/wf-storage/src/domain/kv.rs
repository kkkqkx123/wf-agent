use crate::error::StorageError;
use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait KeyValueStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    async fn set(&self, key: &str, value: &[u8]) -> Result<(), StorageError>;
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
    async fn scan(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, StorageError>;
    async fn clear(&self) -> Result<(), StorageError>;
}

pub struct MemoryKeyValueStore {
    data: RwLock<HashMap<String, Vec<u8>>>,
}

impl MemoryKeyValueStore {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryKeyValueStore {
    fn default() -> Self {
        Self::new()
    }
}

fn read_lock(data: &RwLock<HashMap<String, Vec<u8>>>) -> Result<RwLockReadGuard<'_, HashMap<String, Vec<u8>>>, StorageError> {
    data.read().map_err(|_| StorageError::General {
        operation: "kv_read".to_string(),
        message: "lock poisoned".to_string(),
        source: None,
    })
}

fn write_lock(data: &RwLock<HashMap<String, Vec<u8>>>) -> Result<RwLockWriteGuard<'_, HashMap<String, Vec<u8>>>, StorageError> {
    data.write().map_err(|_| StorageError::General {
        operation: "kv_write".to_string(),
        message: "lock poisoned".to_string(),
        source: None,
    })
}

impl KeyValueStore for MemoryKeyValueStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let data = read_lock(&self.data)?;
        Ok(data.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        let mut data = write_lock(&self.data)?;
        data.insert(key.to_string(), value.to_vec());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        let mut data = write_lock(&self.data)?;
        Ok(data.remove(key).is_some())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let data = read_lock(&self.data)?;
        Ok(data.contains_key(key))
    }

    async fn scan(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        let data = read_lock(&self.data)?;
        let results: Vec<(String, Vec<u8>)> = data
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(results)
    }

    async fn clear(&self) -> Result<(), StorageError> {
        let mut data = write_lock(&self.data)?;
        data.clear();
        Ok(())
    }
}

use async_trait::async_trait;
use serde_json::Value;

use crate::domain::store::{QueryFilter, Store};

/// Adapter that bridges `wf-storage::Store` to `wf-core::PersistableStorage`,
/// unifying the foundation-layer registry persistence with the infra-layer
/// storage backend.
pub struct StorePersistable<S: Store> {
    store: S,
    key_prefix: String,
}

impl<S: Store> StorePersistable<S> {
    pub fn new(store: S, key_prefix: impl Into<String>) -> Self {
        Self {
            store,
            key_prefix: key_prefix.into(),
        }
    }

    fn prefixed_key(&self, key: &str) -> String {
        format!("{}:{}", self.key_prefix, key)
    }

    fn registry_metadata() -> Value {
        serde_json::json!({"entityType": "registry"})
    }
}

#[async_trait]
impl<S: Store> wf_core::registry::PersistableStorage for StorePersistable<S> {
    async fn save(&self, key: &str, data: &[u8]) -> wf_core::registry::RegistryResult<()> {
        let prefixed = self.prefixed_key(key);
        self.store
            .save(&prefixed, data, &Self::registry_metadata())
            .await
            .map_err(|e| wf_core::registry::RegistryError::StorageError {
                message: e.to_string(),
            })
    }

    async fn load(&self, key: &str) -> wf_core::registry::RegistryResult<Option<Vec<u8>>> {
        let prefixed = self.prefixed_key(key);
        self.store
            .load(&prefixed)
            .await
            .map(|opt| opt.map(|(data, _)| data))
            .map_err(|e| wf_core::registry::RegistryError::StorageError {
                message: e.to_string(),
            })
    }

    async fn delete(&self, key: &str) -> wf_core::registry::RegistryResult<()> {
        let prefixed = self.prefixed_key(key);
        self.store
            .delete(&prefixed)
            .await
            .map_err(|e| wf_core::registry::RegistryError::StorageError {
                message: e.to_string(),
            })
    }

    async fn list(&self, prefix: &str) -> wf_core::registry::RegistryResult<Vec<String>> {
        let filter = QueryFilter::new().with_id_prefix(&self.prefixed_key(prefix));
        let entries = self.store.list(Some(&filter)).await.map_err(|e| {
            wf_core::registry::RegistryError::StorageError {
                message: e.to_string(),
            }
        })?;
        let prefix_len = self.key_prefix.len() + 1; // "prefix:"
        Ok(entries
            .into_iter()
            .map(|(id, _)| id[prefix_len..].to_string())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::memory::MemoryStorage;
    use wf_core::registry::PersistableStorage;

    #[tokio::test]
    async fn test_store_persistable_roundtrip() {
        let store = MemoryStorage::new("registry_test");
        let bridge = StorePersistable::new(store, "reg");

        bridge.save("key1", b"hello").await.unwrap();
        let loaded = bridge.load("key1").await.unwrap();
        assert_eq!(loaded.as_deref(), Some(b"hello".as_ref()));

        bridge.delete("key1").await.unwrap();
        assert!(bridge.load("key1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_store_persistable_list() {
        let store = MemoryStorage::new("registry_test");
        let bridge = StorePersistable::new(store, "reg");

        bridge.save("a/1", b"data1").await.unwrap();
        bridge.save("a/2", b"data2").await.unwrap();
        bridge.save("b/1", b"data3").await.unwrap();

        let keys = bridge.list("a/").await.unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"a/1".to_string()));
        assert!(keys.contains(&"a/2".to_string()));
    }
}

use std::marker::PhantomData;

use serde_json::Value;

use crate::domain::entity::Entity;
use crate::domain::store::{BatchItem, BatchStore, QueryFilter, Store};
use crate::util::compression::{maybe_compress, maybe_decompress};
use crate::util::hash::compute_hash;
use crate::error::StorageError;

pub struct EntityStore<S, T> {
    storage: S,
    _marker: PhantomData<T>,
}

impl<S, T> EntityStore<S, T>
where
    S: Store,
    T: Entity,
{
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            _marker: PhantomData,
        }
    }

    pub fn into_inner(self) -> S {
        self.storage
    }

    pub fn inner(&self) -> &S {
        &self.storage
    }

    pub async fn save(&self, entity: &T) -> Result<(), StorageError> {
        let id = entity.entity_id().to_string();
        let metadata_json = serde_json::to_value(entity.metadata())?;
        let data = entity.to_bytes()?;
        let (compressed, was_compressed) = maybe_compress(&data)?;

        let mut full_metadata = serde_json::json!({
            "entityType": T::entity_type(),
            "compressed": was_compressed,
        });

        if let Value::Object(mut map) = full_metadata {
            if let Value::Object(meta_map) = metadata_json {
                map.extend(meta_map);
            }
            full_metadata = Value::Object(map);
        }

        self.storage.save(&id, &compressed, &full_metadata).await
    }

    pub async fn load(&self, id: &str) -> Result<Option<T>, StorageError> {
        match self.storage.load(id).await? {
            Some((data, metadata)) => {
                let compressed = metadata
                    .get("compressed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let decompressed = maybe_decompress(&data, compressed)?;
                T::from_bytes(&decompressed).map(Some)
            }
            None => Ok(None),
        }
    }

    pub async fn delete(&self, id: &str) -> Result<(), StorageError> {
        self.storage.delete(id).await
    }

    pub async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<T>, StorageError> {
        let entries = self.storage.list_data(filter).await?;
        let mut results = Vec::with_capacity(entries.len());
        for (data, metadata) in entries {
            let compressed = metadata
                .get("compressed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let decompressed = maybe_decompress(&data, compressed)?;
            if let Some(entity) = T::from_bytes(&decompressed).map(Some)? {
                results.push(entity);
            }
        }
        Ok(results)
    }

    pub async fn list_metadata(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError> {
        self.storage.list(filter).await
    }

    pub async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        self.storage.exists(id).await
    }

    pub async fn clear(&self) -> Result<(), StorageError> {
        self.storage.clear().await
    }

    pub async fn compute_hash(&self, id: &str) -> Result<Option<String>, StorageError> {
        match self.storage.load(id).await? {
            Some((data, _)) => Ok(Some(compute_hash(&data))),
            None => Ok(None),
        }
    }
}

impl<S, T> EntityStore<S, T>
where
    S: Store + BatchStore,
    T: Entity,
{
    pub async fn save_batch(&self, entities: &[T]) -> Result<(), StorageError> {
        let items: Result<Vec<BatchItem>, StorageError> = entities
            .iter()
            .map(|e| {
                let metadata_json = serde_json::to_value(e.metadata())?;
                let data = e.to_bytes()?;
                let (compressed, was_compressed) = maybe_compress(&data)?;

                let mut full_metadata = serde_json::json!({
                    "entityType": T::entity_type(),
                    "compressed": was_compressed,
                });

                if let Value::Object(mut map) = full_metadata {
                    if let Value::Object(meta_map) = metadata_json {
                        map.extend(meta_map);
                    }
                    full_metadata = Value::Object(map);
                }

                Ok(BatchItem::new(e.entity_id().to_string(), compressed, full_metadata))
            })
            .collect();
        self.storage.save_batch(&items?).await
    }

    pub async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        self.storage.delete_batch(ids).await
    }
}



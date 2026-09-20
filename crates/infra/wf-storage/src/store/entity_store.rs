use std::collections::HashMap;
use std::marker::PhantomData;

use serde_json::Value;

use crate::domain::entity::Entity;
use crate::domain::store::{BatchItem, QueryFilter, Store, StoreExt};
use crate::error::StorageError;
use crate::util::compression::{maybe_compress_with_threshold, maybe_decompress};
use crate::util::hash::compute_hash;

/// Default compression threshold: payloads smaller than this are stored as-is.
const DEFAULT_COMPRESSION_THRESHOLD: usize = 1024;

#[derive(Clone)]
pub struct EntityStore<S, T> {
    storage: S,
    compression_threshold: usize,
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
            compression_threshold: DEFAULT_COMPRESSION_THRESHOLD,
            _marker: PhantomData,
        }
    }

    pub fn with_compression_threshold(mut self, threshold: usize) -> Self {
        self.compression_threshold = threshold;
        self
    }

    pub fn into_inner(self) -> S {
        self.storage
    }

    pub fn inner(&self) -> &S {
        &self.storage
    }

    /// Serialize one entity into its storage row: compressed payload plus
    /// metadata merged with the entity-type marker and compression flag.
    /// Shared by single saves and batch saves so both paths encode rows
    /// identically.
    fn encode_item(&self, entity: &T) -> Result<BatchItem, StorageError> {
        let metadata_json = serde_json::to_value(entity.metadata())?;
        let data = entity.to_bytes()?;
        let (compressed, was_compressed) =
            maybe_compress_with_threshold(&data, self.compression_threshold)?;

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

        Ok(BatchItem::new(
            entity.entity_id().to_string(),
            compressed,
            full_metadata,
        ))
    }

    pub async fn save(&self, entity: &T) -> Result<(), StorageError> {
        let item = self.encode_item(entity)?;
        self.storage
            .save(&item.id, &item.data, &item.metadata)
            .await
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

    pub async fn list(&self, filter: Option<&QueryFilter>) -> Result<Vec<T>, StorageError> {
        let (entities, corrupt_count) = self.list_with_corruption(filter).await?;
        if corrupt_count > 0 {
            tracing::warn!(
                entity_type = T::entity_type(),
                corrupt_count,
                "list skipped corrupted records"
            );
        }
        Ok(entities)
    }

    /// List entities matching `filter`, returning the deserialized entities and
    /// the number of records that failed deserialization. Failed records are
    /// skipped, logged at warn level and counted; no status marker is written
    /// back, so callers observe corruption through the returned count.
    pub async fn list_with_corruption(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<(Vec<T>, u64), StorageError> {
        let entries = self.storage.list_data(filter).await?;
        let mut results = Vec::with_capacity(entries.len());
        let mut corrupt_count = 0u64;
        for (data, metadata) in entries {
            let compressed = metadata
                .get("compressed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let decompressed = maybe_decompress(&data, compressed)?;
            match T::from_bytes(&decompressed) {
                Ok(entity) => results.push(entity),
                Err(e) => {
                    corrupt_count += 1;
                    tracing::warn!(
                        entity_type = T::entity_type(),
                        error = %e,
                        "corrupted record skipped during list"
                    );
                }
            }
        }
        Ok((results, corrupt_count))
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

    /// Count records matching a filter without loading their payloads. The
    /// backend overrides this with an aggregate `COUNT(*)` query where
    /// available.
    pub async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        self.storage.count(filter).await
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

    /// Read-modify-write of an entity by id without locking: the current
    /// record is loaded, `f` mutates it in memory, and the result is saved
    /// back. Concurrent mutations of the same id must be serialized by the
    /// caller. Only low-contention management updates may use this helper;
    /// high-contention entities must serialize callers or use an atomic batch
    /// instead. Returns `Ok(None)` when no record with the id exists.
    pub async fn mutate(
        &self,
        id: &str,
        f: impl FnOnce(&mut T) -> Result<(), StorageError>,
    ) -> Result<Option<T>, StorageError> {
        let mut entity = match self.load(id).await? {
            Some(entity) => entity,
            None => return Ok(None),
        };
        f(&mut entity)?;
        self.save(&entity).await?;
        Ok(Some(entity))
    }
}

impl<S, T> EntityStore<S, T>
where
    S: Store + StoreExt,
    T: Entity,
{
    /// Count records grouped by a metadata field (delegates to the backend).
    pub async fn count_by_field(&self, field: &str) -> Result<HashMap<String, u64>, StorageError> {
        self.storage.count_by_field(field).await
    }

    pub async fn save_batch(&self, entities: &[T]) -> Result<(), StorageError> {
        let items: Result<Vec<BatchItem>, StorageError> =
            entities.iter().map(|e| self.encode_item(e)).collect();
        self.storage.save_batch(&items?).await
    }

    pub async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        self.storage.delete_batch(ids).await
    }
}

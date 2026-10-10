//! Storage-backed checkpoint state manager.
//!
//! Responsibilities are split across sibling modules:
//! - `metadata`: indexed metadata document building and payload field reads
//! - `chain`: delta chain derivation and compaction
//! - `cleanup`: per-entity cleanup with persisted watermark
//! - `query`: batch and parent-scoped latest-metadata resolution
//! - `loader`: raw data and metadata loading for delta restoration
//!
//! This module owns the manager contract: construction, the storage-backed
//! implementation of `CheckpointStateManager`, and single-row reads.

use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Instant;

use checkpoint_base::cleanup_policy::CleanupStrategy;
use checkpoint_base::clock::CheckpointClock;
use checkpoint_base::delta::CheckpointLoader;
use checkpoint_base::error::CheckpointError;
use checkpoint_base::serializer::{CheckpointCodec, CheckpointSerializer};
use serde::Serialize;
use wf_metrics::CheckpointMetricsCollector;
use wf_storage::backend::StorageBackend;
use wf_storage::domain::store::{QueryFilter, Store, StoreExt};
use wf_storage::error::StorageError;
use wf_types::checkpoint::{CheckpointType, CompressionStrategy};
use wf_types::storage::CheckpointStorageMetadata;

pub use metadata::parse_storage_metadata;

mod chain;
mod cleanup;
mod loader;
mod metadata;
mod query;
#[cfg(test)]
mod tests;

use metadata::MetadataArgs;

/// Generic checkpoint state manager persisting checkpoints into the shared
/// storage backend. Serialization is delegated to the checkpoint serializer
/// and every indexed read is served by one storage document.
pub struct StorageBackedStateManager<T> {
    storage: Arc<StorageBackend>,
    metrics: Option<Arc<CheckpointMetricsCollector>>,
    /// Per-entity cleanup mutexes so concurrent cleanup runs for the same
    /// entity are serialized.
    cleanup_locks: dashmap::DashMap<String, Arc<tokio::sync::Mutex<()>>>,
    /// Time source for save-timestamp defaults and cleanup watermark
    /// clamping. Tests inject a manual clock and advance it explicitly.
    clock: CheckpointClock,
    /// Compression applied on the save path. Reads detect gzip via magic
    /// bytes, so switching strategies is format-compatible.
    compression: CompressionStrategy,
    _marker: PhantomData<T>,
}

impl<T> StorageBackedStateManager<T> {
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self {
            storage,
            metrics: None,
            cleanup_locks: dashmap::DashMap::new(),
            clock: CheckpointClock::system(),
            compression: CompressionStrategy::Auto,
            _marker: PhantomData,
        }
    }

    pub fn with_metrics(mut self, metrics: Arc<CheckpointMetricsCollector>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Drive save-timestamp defaults and cleanup watermark clamping from an
    /// explicit clock instead of the system clock.
    pub fn with_clock(mut self, clock: CheckpointClock) -> Self {
        self.clock = clock;
        self
    }

    /// Override the save-path compression strategy (default `Auto`).
    pub fn with_compression(mut self, compression: CompressionStrategy) -> Self {
        self.compression = compression;
        self
    }

    /// Override the save-path compression strategy in place, so owners that
    /// already hold the manager (e.g. coordinators learning the strategy
    /// after construction) can apply it without rebuilding.
    pub fn set_compression(&mut self, compression: CompressionStrategy) {
        self.compression = compression;
    }

    pub fn compression(&self) -> CompressionStrategy {
        self.compression
    }

    pub fn clock(&self) -> &CheckpointClock {
        &self.clock
    }

    /// The underlying storage backend (used to rebuild state managers in
    /// spawned restore tasks).
    pub fn storage(&self) -> &Arc<StorageBackend> {
        &self.storage
    }
}

impl<T> crate::state::CheckpointStateManager for StorageBackedStateManager<T>
where
    T: Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    type Checkpoint = T;

    async fn save(
        &self,
        checkpoint: &Self::Checkpoint,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<(), CheckpointError> {
        let start = Instant::now();
        // Every indexed field below is read off one serialization of the
        // payload: the save path walks a large snapshot several times and
        // re-encoding it per field dominated the write.
        let payload = metadata::payload_value(checkpoint)?;
        let id = metadata::field_as_str(&payload, "id")?;
        let checkpoint_type = metadata::checkpoint_type_of(&payload);
        let is_full = checkpoint_type == CheckpointType::Full;
        let timestamp = metadata::optional_i64_field(&payload, "timestamp")
            .or_else(|| self.clock.now_ms())
            .ok_or_else(|| {
                CheckpointError::Internal(
                    "checkpoint has no timestamp and the checkpoint clock is unavailable"
                        .to_string(),
                )
            })?;
        let base_checkpoint_id =
            metadata::optional_field_as_str(&payload, "baseCheckpointId", "base_checkpoint_id");
        let previous_checkpoint_id = metadata::optional_field_as_str(
            &payload,
            "previousCheckpointId",
            "previous_checkpoint_id",
        );

        // Compression follows the configured strategy (`Auto` by default:
        // payloads larger than the compression threshold are gzip
        // compressed; smaller payloads stay plain). Reads transparently
        // detect gzip via magic bytes, so the switch is format-compatible.
        // The async variant keeps the compressor's deflate working set off
        // the caller's stack (checkpoint saves sit deep inside nested
        // execution futures).
        let data = CheckpointSerializer::serialize_with_compression_async(
            checkpoint,
            CheckpointCodec::Json,
            self.compression,
        )
        .await?;

        let (chain_root_id, chain_position) = self
            .compute_chain_info(&id, &checkpoint_type, previous_checkpoint_id.as_deref())
            .await?;

        let metadata = self.build_metadata(MetadataArgs {
            id: &id,
            entity_type,
            entity_id,
            parent_entity_id: metadata::parent_entity_id_of(&payload).as_deref(),
            checkpoint_type,
            timestamp,
            base_checkpoint_id: base_checkpoint_id.as_deref(),
            previous_checkpoint_id: previous_checkpoint_id.as_deref(),
            chain_root_id: chain_root_id.as_deref(),
            chain_position,
            blob_size: data.len() as u64,
            compressed: CheckpointSerializer::is_compressed(&data),
            tags: metadata::tags_of(&payload).as_ref(),
            custom_fields: metadata::custom_fields_of(&payload).as_ref(),
        });

        self.storage
            .save(&id, &data, &metadata)
            .await
            .map_err(CheckpointError::Storage)?;

        if let Some(ref metrics) = self.metrics {
            metrics.record_creation(
                entity_id,
                start.elapsed().as_millis() as f64,
                data.len() as u64,
                is_full,
            );
            metrics.record_chain_length(entity_id, (chain_position.unwrap_or(0) + 1) as u64);
        }

        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<Self::Checkpoint>, CheckpointError> {
        let start = Instant::now();
        match self
            .storage
            .load(id)
            .await
            .map_err(CheckpointError::Storage)
        {
            Err(CheckpointError::Storage(StorageError::Integrity { .. })) => {
                // The payload failed its integrity check. Mark the record so
                // queries and recovery can see it is unusable, then surface
                // the corruption. Best-effort: a failed metadata update must
                // not mask the corruption error itself.
                if let Err(mark_err) = self.storage.update_status(id, "corrupted").await {
                    tracing::warn!(
                        checkpoint_id = %id,
                        error = %mark_err,
                        "Failed to mark corrupt checkpoint"
                    );
                }
                Err(CheckpointError::Corrupted {
                    id: id.to_string(),
                    reason: "checkpoint payload failed integrity verification".to_string(),
                })
            }
            result => {
                let Some((data, _)) = result? else {
                    if let Some(ref metrics) = self.metrics {
                        metrics.record_load(id, start.elapsed().as_millis() as f64, false);
                    }
                    return Ok(None);
                };
                let checkpoint = CheckpointSerializer::auto_deserialize(&data)?;
                if let Some(ref metrics) = self.metrics {
                    metrics.record_load(id, start.elapsed().as_millis() as f64, true);
                }
                Ok(Some(checkpoint))
            }
        }
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<Option<Self::Checkpoint>>, CheckpointError> {
        let mut result = Vec::with_capacity(ids.len());
        for id in ids {
            result.push(self.load(id).await?);
        }
        Ok(result)
    }

    async fn delete(&self, id: &str) -> Result<bool, CheckpointError> {
        let exists = self
            .storage
            .exists(id)
            .await
            .map_err(CheckpointError::Storage)?;
        if exists {
            self.storage
                .delete(id)
                .await
                .map_err(CheckpointError::Storage)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        let filter = QueryFilter::new().with_field("entityId", entity_id);

        let entries = self
            .storage
            .list(Some(&filter))
            .await
            .map_err(CheckpointError::Storage)?;

        let mut results: Vec<CheckpointStorageMetadata> = Vec::new();
        for (id, meta) in entries {
            match parse_storage_metadata(&id, entity_id, &meta) {
                Ok(parsed) => results.push(parsed),
                Err(e) => {
                    tracing::warn!(checkpoint_id = %id, error = %e, "skipping corrupt checkpoint metadata")
                }
            }
        }

        results.sort_by(|a, b| (a.timestamp, &a.id).cmp(&(b.timestamp, &b.id)));
        Ok(results)
    }

    /// Paged listing in the same ascending timestamp order as the full
    /// listing: offset/limit are pushed down to the storage backend so only
    /// the requested window is scanned.
    async fn list_by_entity_paged(
        &self,
        entity_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let filter = QueryFilter::new()
            .with_field("entityId", entity_id)
            .with_order_by("timestamp", false)
            .with_offset(offset)
            .with_limit(limit);

        let entries = self
            .storage
            .list(Some(&filter))
            .await
            .map_err(CheckpointError::Storage)?;

        let mut results: Vec<CheckpointStorageMetadata> = Vec::new();
        for (id, meta) in entries {
            match parse_storage_metadata(&id, entity_id, &meta) {
                Ok(parsed) => results.push(parsed),
                Err(e) => {
                    tracing::warn!(checkpoint_id = %id, error = %e, "skipping corrupt checkpoint metadata")
                }
            }
        }
        results.sort_by(|a, b| (a.timestamp, &a.id).cmp(&(b.timestamp, &b.id)));
        Ok(results)
    }

    /// Aggregate count for an entity, pushed down to a `COUNT(*)` query
    /// instead of materializing the full metadata listing.
    async fn count_by_entity(&self, entity_id: &str) -> Result<u64, CheckpointError> {
        let filter = QueryFilter::new().with_field("entityId", entity_id);
        self.storage
            .count(Some(&filter))
            .await
            .map_err(CheckpointError::Storage)
    }

    /// Latest checkpoint metadata for an entity, resolved with an
    /// `ORDER BY timestamp DESC LIMIT 1` query instead of scanning the whole
    /// history (eliminates the O(N) list + pop on the hot path).
    async fn get_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        let filter = QueryFilter::new()
            .with_field("entityId", entity_id)
            .with_order_by("timestamp", true)
            .with_limit(1);

        let entries = self
            .storage
            .list(Some(&filter))
            .await
            .map_err(CheckpointError::Storage)?;

        let mut iter = entries.into_iter();
        let Some((id, meta)) = iter.next() else {
            return Ok(None);
        };
        Ok(Some(parse_storage_metadata(&id, entity_id, &meta)?))
    }

    async fn load_metadata(
        &self,
        id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        CheckpointLoader::load_metadata(self, id).await
    }

    async fn cleanup(
        &self,
        entity_id: &str,
        max_count: Option<u32>,
    ) -> Result<u64, CheckpointError> {
        let max = match max_count {
            Some(0) => return Ok(0),
            Some(n) => n as u64,
            None => return Ok(0),
        };

        self.cleanup_with_strategy(
            entity_id,
            &CleanupStrategy::CountBased {
                max_checkpoints: max,
                min_retention: 1,
            },
        )
        .await
    }

    async fn cleanup_with_strategy(
        &self,
        entity_id: &str,
        strategy: &CleanupStrategy,
    ) -> Result<u64, CheckpointError> {
        let entity_type = self
            .get_latest(entity_id)
            .await?
            .map(|m| m.entity_type)
            .unwrap_or_else(|| "unknown".to_string());
        let result = self
            .execute_cleanup_for_entity(entity_id, &entity_type, None, strategy)
            .await?;
        Ok(result.deleted_count)
    }
}

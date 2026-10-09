use crate::state::CheckpointStateManager;
use checkpoint_base::cleanup_policy::{CleanupExecutor, CleanupResult, CleanupStrategy};
use checkpoint_base::clock::CheckpointClock;
use checkpoint_base::delta::{CheckpointLoader, DiffCalculator};
use checkpoint_base::error::CheckpointError;
use checkpoint_base::serializer::{CheckpointCodec, CheckpointSerializer};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use wf_metrics::CheckpointMetricsCollector;
use wf_storage::backend::StorageBackend;
use wf_storage::domain::store::{BatchItem, QueryFilter, Store, StoreExt, StoreOperation};
use wf_storage::error::StorageError;
use wf_types::checkpoint::CheckpointType;
use wf_types::checkpoint::CompressionStrategy;
use wf_types::storage::CheckpointStorageMetadata;

/// Reserved record key prefix for per-entity cleanup metadata (watermark).
/// The record's metadata carries no `entityId`, so it never matches
/// `list_by_entity` filters.
const ENTITY_CLEANUP_META_KEY_PREFIX: &str = "__checkpoint_cleanup_meta__:";

/// Every Nth cleanup run is a full scan.
const FULL_SCAN_INTERVAL: u64 = 10;

pub struct StorageBackedStateManager<T> {
    storage: Arc<StorageBackend>,
    metrics: Option<Arc<CheckpointMetricsCollector>>,
    /// Per-entity cleanup mutexes so concurrent cleanup runs for the same
    /// entity are serialized.
    cleanup_locks: dashmap::DashMap<String, Arc<tokio::sync::Mutex<()>>>,
    /// Time source for save-timestamp defaults and cleanup watermark
    /// clamping. Tests inject a manual clock and advance it explicitly.
    clock: CheckpointClock,
    _marker: std::marker::PhantomData<T>,
}

impl<T> StorageBackedStateManager<T> {
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self {
            storage,
            metrics: None,
            cleanup_locks: dashmap::DashMap::new(),
            clock: CheckpointClock::system(),
            _marker: std::marker::PhantomData,
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

    /// The underlying storage backend (used to rebuild state managers in
    /// spawned restore tasks).
    pub fn storage(&self) -> &Arc<StorageBackend> {
        &self.storage
    }

    fn entity_cleanup_meta_key(entity_id: &str) -> String {
        format!("{ENTITY_CLEANUP_META_KEY_PREFIX}{entity_id}")
    }

    /// Load the persisted cleanup watermark for an entity.
    /// Returns `(last_watermark, run_count)`.
    async fn load_entity_cleanup_metadata(
        &self,
        entity_id: &str,
    ) -> Result<(Option<i64>, u64), CheckpointError> {
        let key = Self::entity_cleanup_meta_key(entity_id);
        match self
            .storage
            .load(&key)
            .await
            .map_err(CheckpointError::Storage)?
        {
            Some((_, meta)) => {
                let watermark = meta.get("cleanupWatermark").and_then(|v| v.as_i64());
                let run_count = meta
                    .get("cleanupRunCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                Ok((watermark, run_count))
            }
            None => Ok((None, 0)),
        }
    }

    /// Build the watermark record for an entity (written together with the
    /// cleanup deletes through `Store::apply_batch`).
    fn entity_cleanup_metadata_item(
        &self,
        entity_id: &str,
        watermark: i64,
        run_count: u64,
    ) -> BatchItem {
        let key = Self::entity_cleanup_meta_key(entity_id);
        let metadata = serde_json::json!({
            "cleanupWatermark": watermark,
            "cleanupRunCount": run_count,
        });
        BatchItem::new(key, Vec::new(), metadata)
    }

    /// Build the indexed metadata document for one checkpoint row. The record
    /// type owns the key set; `compressed` describes the encoded blob, which
    /// only this writer knows.
    fn build_metadata(&self, args: MetadataArgs<'_>) -> Value {
        let record = CheckpointStorageMetadata {
            id: args.id.to_string(),
            entity_type: args.entity_type.to_string(),
            entity_id: args.entity_id.to_string(),
            parent_entity_id: args.parent_entity_id.map(String::from),
            checkpoint_type: args.checkpoint_type,
            timestamp: args.timestamp,
            // A row is only written once its payload is stored, so the write
            // itself has completed.
            status: wf_types::checkpoint::CheckpointStatus::Completed,
            previous_checkpoint_id: args.previous_checkpoint_id.map(String::from),
            base_checkpoint_id: args.base_checkpoint_id.map(String::from),
            chain_root_id: args.chain_root_id.map(String::from),
            chain_position: args.chain_position,
            blob_size: Some(args.blob_size),
            tags: args.tags.cloned(),
            custom_fields: args.custom_fields.cloned(),
        };
        let mut metadata = record.metadata_document();
        if let Some(map) = metadata.as_object_mut() {
            map.insert("compressed".into(), Value::Bool(args.compressed));
        }
        metadata
    }
}

/// Aggregated fields for building a checkpoint storage metadata document.
struct MetadataArgs<'a> {
    id: &'a str,
    entity_type: &'a str,
    entity_id: &'a str,
    parent_entity_id: Option<&'a str>,
    checkpoint_type: CheckpointType,
    timestamp: i64,
    base_checkpoint_id: Option<&'a str>,
    previous_checkpoint_id: Option<&'a str>,
    chain_root_id: Option<&'a str>,
    chain_position: Option<u32>,
    blob_size: u64,
    compressed: bool,
    tags: Option<&'a Vec<String>>,
    custom_fields: Option<&'a wf_types::Metadata>,
}

impl<T> StorageBackedStateManager<T>
where
    T: Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    async fn compute_chain_info(
        &self,
        id: &str,
        checkpoint_type: &CheckpointType,
        previous_checkpoint_id: Option<&str>,
    ) -> Result<(Option<String>, Option<u32>), CheckpointError> {
        match checkpoint_type {
            CheckpointType::Full => Ok((Some(id.to_string()), Some(0))),
            CheckpointType::Delta => match previous_checkpoint_id {
                Some(prev) => match CheckpointLoader::load_metadata(self, prev).await? {
                    Some(meta) => Ok((
                        meta.chain_root_id.or_else(|| Some(prev.to_string())),
                        Some(meta.chain_position.unwrap_or(0) + 1),
                    )),
                    // A delta must link to a readable predecessor. Falling
                    // back to a new chain head would silently hide a broken
                    // chain, so report the missing link loudly.
                    None => Err(CheckpointError::DeltaChainBroken {
                        checkpoint_id: id.to_string(),
                        missing_id: prev.to_string(),
                    }),
                },
                None => Err(CheckpointError::Validation {
                    reason: format!("delta checkpoint '{id}' missing previous_checkpoint_id"),
                }),
            },
        }
    }

    /// Resolve the latest checkpoint metadata for many entities in a single
    /// storage query (IN filter), deduplicated per entity. This eliminates
    /// the N+1 `get_latest` pattern in child hierarchy restore.
    pub async fn list_latest_by_entities(
        &self,
        entity_ids: &[String],
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        if entity_ids.is_empty() {
            return Ok(Vec::new());
        }
        let filter = QueryFilter::new()
            .with_field_in("entityId", entity_ids.to_vec())
            .with_order_by("timestamp", true);

        let entries = self
            .storage
            .list(Some(&filter))
            .await
            .map_err(CheckpointError::Storage)?;

        // Keep only the newest record per entity (list is timestamp-descending).
        // Corrupt rows are skipped loudly so one bad row never fails the batch.
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut latest: Vec<CheckpointStorageMetadata> = Vec::new();
        for (id, meta) in entries {
            let entity_id = meta
                .get("entityId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if seen.insert(entity_id.clone()) {
                match parse_storage_metadata(&id, &entity_id, &meta) {
                    Ok(parsed) => latest.push(parsed),
                    Err(e) => {
                        tracing::warn!(checkpoint_id = %id, error = %e, "skipping corrupt checkpoint metadata")
                    }
                }
            }
        }
        Ok(latest)
    }

    /// Resolve the latest checkpoint metadata of every entity spawned directly
    /// from `parent_entity_id`. The link lives on the child, so the answer is
    /// current even when the parent last persisted before the child existed.
    ///
    /// Two constant queries rather than one: only a full checkpoint carries a
    /// snapshot, so the indexed `parentEntityId` scan identifies *which*
    /// entities are children, and the per-child latest lookup then returns
    /// each child's newest row whatever its type. Reading the latest row out
    /// of the link scan instead would hand back the newest full checkpoint of
    /// a child whose newest checkpoint is a delta.
    pub async fn list_latest_by_parent(
        &self,
        parent_entity_id: &str,
    ) -> Result<Vec<CheckpointStorageMetadata>, CheckpointError> {
        let linked = self
            .storage
            .list(Some(
                &QueryFilter::new().with_field("parentEntityId", parent_entity_id),
            ))
            .await
            .map_err(CheckpointError::Storage)?;

        let mut child_ids: Vec<String> = Vec::new();
        for (_, meta) in linked {
            let Some(entity_id) = meta.get("entityId").and_then(|v| v.as_str()) else {
                continue;
            };
            if !child_ids.iter().any(|id| id == entity_id) {
                child_ids.push(entity_id.to_string());
            }
        }
        if child_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.list_latest_by_entities(&child_ids).await
    }

    /// Execute a cleanup run for an entity with dependency protection:
    /// per-entity cleanup serialization, optional excluded checkpoint id,
    /// real `blob_size`-based freed byte accounting, and a `CleanupResult`.
    ///
    /// Incremental semantics: every 10th run is a full scan;
    /// otherwise only checkpoints newer than the persisted watermark (plus
    /// the excluded id) are considered candidates. After a run the watermark
    /// advances to the newest considered timestamp clamped to now and the
    /// run count always advances so empty rounds never stall the full scan.
    /// Late records older than the watermark stay invisible until the next
    /// full scan.
    pub async fn execute_cleanup_for_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
        exclude_checkpoint_id: Option<&str>,
        strategy: &CleanupStrategy,
    ) -> Result<CleanupResult, CheckpointError> {
        // Entity type is kept for observability and future per-type policy
        // routing. Candidate selection filters by entity id; stored rows carry
        // their own type and mismatches are tolerated here.
        let _ = entity_type;
        // Serialize cleanup runs per entity.
        let lock = self
            .cleanup_locks
            .entry(entity_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        let all = self.list_by_entity(entity_id).await?;
        let (last_watermark, run_count) = self.load_entity_cleanup_metadata(entity_id).await?;
        let is_full_scan = run_count % FULL_SCAN_INTERVAL == 0;

        // Incremental filtering: only consider checkpoints created after the
        // watermark (plus the excluded id, which must stay visible).
        let candidates: Vec<CheckpointStorageMetadata> = match last_watermark {
            Some(watermark) if !is_full_scan => all
                .iter()
                .filter(|c| c.timestamp > watermark || Some(c.id.as_str()) == exclude_checkpoint_id)
                .cloned()
                .collect(),
            _ => all.clone(),
        };

        let executor = CleanupExecutor::new();
        let mut result =
            executor.evaluate_protected_with_result(&candidates, strategy, &self.clock);

        if let Some(exclude) = exclude_checkpoint_id {
            result.deleted_checkpoint_ids.retain(|id| id != exclude);
            result.deleted_count = result.deleted_checkpoint_ids.len() as u64;
            let size_by_id: HashMap<&str, u64> = candidates
                .iter()
                .map(|c| (c.id.as_str(), c.blob_size.unwrap_or(0)))
                .collect();
            result.freed_bytes = result
                .deleted_checkpoint_ids
                .iter()
                .map(|id| size_by_id.get(id.as_str()).copied().unwrap_or(0))
                .sum();
            result.remaining_count = candidates.len() as u64 - result.deleted_count;
        }

        let start = Instant::now();

        // the deletes and the watermark advance land in one atomic
        // batch (Sqlite/PostgreSQL run it inside BEGIN/COMMIT). Without the
        // transaction a crash between the two leaves either re-deleted
        // checkpoints or — worse — a watermark that skips surviving
        // checkpoints in the next incremental run (history gap).
        let survivors: Vec<&CheckpointStorageMetadata> = candidates
            .iter()
            .filter(|c| !result.deleted_checkpoint_ids.contains(&c.id))
            .collect();
        let mut operations: Vec<StoreOperation> = Vec::new();
        let mut deleted = 0u64;
        for id in &result.deleted_checkpoint_ids {
            if self
                .storage
                .exists(id)
                .await
                .map_err(CheckpointError::Storage)?
            {
                operations.push(StoreOperation::Delete(id.clone()));
                deleted += 1;
            }
        }
        if !all.is_empty() {
            let now = self.clock.now_ms().ok_or_else(|| {
                CheckpointError::Internal(
                    "checkpoint clock unavailable; refusing to advance cleanup watermark"
                        .to_string(),
                )
            })?;
            let next_watermark = survivors
                .iter()
                .map(|c| c.timestamp)
                .max()
                .or_else(|| candidates.iter().map(|c| c.timestamp).max())
                .or(last_watermark)
                .unwrap_or(now)
                .min(now);
            operations.push(StoreOperation::Save(self.entity_cleanup_metadata_item(
                entity_id,
                next_watermark,
                run_count + 1,
            )));
        }
        if !operations.is_empty() {
            self.storage
                .apply_batch(&operations)
                .await
                .map_err(CheckpointError::Storage)?;
        }
        result.deleted_count = deleted;
        result.remaining_count = candidates.len().saturating_sub(deleted as usize) as u64;

        if let Some(ref metrics) = self.metrics {
            metrics.record_cleanup(
                deleted,
                result.freed_bytes,
                start.elapsed().as_millis() as f64,
            );
        }

        // Reap the per-entity lock when no other run (or waiter) holds a
        // reference, so the cleanup_locks map does not grow unboundedly with
        // the number of entities over the process lifetime.
        drop(_guard);
        if Arc::strong_count(&lock).saturating_sub(1) == 1 {
            // Drop the shard read-`Ref` before removing: `DashMap` is not
            // reentrant, so holding the `Ref` while calling `remove` on the
            // same shard self-deadlocks.
            let same = self
                .cleanup_locks
                .get(entity_id)
                .is_some_and(|current| Arc::ptr_eq(current.value(), &lock));
            if same {
                self.cleanup_locks.remove(entity_id);
            }
        }

        Ok(result)
    }

    /// Compact the current delta chain by merging the oldest consecutive delta
    /// pairs until the chain has at most `max_deltas` entries. Merged deltas
    /// are rebased directly on the FULL anchor, the successor's
    /// `previous_checkpoint_id` is fixed up, and the merged-away checkpoints
    /// are deleted. Returns the number of merged checkpoints.
    pub async fn compact_delta_chain<SS, DS>(
        &self,
        entity_id: &str,
        entity_type: &str,
        calculator: &dyn DiffCalculator<SS, DS>,
        max_deltas: u32,
    ) -> Result<u64, CheckpointError>
    where
        SS: Serialize + serde::de::DeserializeOwned + Send + Sync,
        DS: Serialize + serde::de::DeserializeOwned + Send + Sync,
    {
        if max_deltas == 0 {
            return Ok(0);
        }

        let mut chain: Vec<CheckpointStorageMetadata> = Vec::new();
        let mut anchor_id: Option<String> = None;
        let mut current = self.get_latest(entity_id).await?;
        let mut guard = 0u32;

        while let Some(meta) = current {
            guard += 1;
            if guard > 10_000 {
                return Err(CheckpointError::Validation {
                    reason: "delta chain too long or cyclic".to_string(),
                });
            }
            if meta.checkpoint_type == CheckpointType::Full {
                anchor_id = Some(meta.id.clone());
                break;
            }
            chain.push(meta.clone());
            current = match &meta.previous_checkpoint_id {
                Some(prev) => CheckpointLoader::load_metadata(self, prev).await?,
                None => None,
            };
        }
        chain.reverse();

        let mut merged_count = 0u64;

        while chain.len() > max_deltas as usize {
            let anchor_id = anchor_id
                .as_deref()
                .ok_or_else(|| CheckpointError::Validation {
                    reason: "no FULL anchor found for delta chain compaction".to_string(),
                })?;

            let d1 = &chain[0];
            let d2 = &chain[1];

            let anchor_value =
                serde_json::to_value(self.load(anchor_id).await?.ok_or_else(|| {
                    CheckpointError::NotFound {
                        id: anchor_id.to_string(),
                    }
                })?)?;
            let base: SS =
                serde_json::from_value(anchor_value.get("snapshot").cloned().ok_or_else(
                    || CheckpointError::Validation {
                        reason: "anchor checkpoint has no snapshot".to_string(),
                    },
                )?)?;

            let d1_value = serde_json::to_value(
                self.load(&d1.id)
                    .await?
                    .ok_or_else(|| CheckpointError::NotFound { id: d1.id.clone() })?,
            )?;
            let d2_value = serde_json::to_value(
                self.load(&d2.id)
                    .await?
                    .ok_or_else(|| CheckpointError::NotFound { id: d2.id.clone() })?,
            )?;

            let first: DS =
                serde_json::from_value(d1_value.get("delta").cloned().ok_or_else(|| {
                    CheckpointError::Validation {
                        reason: format!("delta checkpoint {} has no delta", d1.id),
                    }
                })?)?;
            let second: DS =
                serde_json::from_value(d2_value.get("delta").cloned().ok_or_else(|| {
                    CheckpointError::Validation {
                        reason: format!("delta checkpoint {} has no delta", d2.id),
                    }
                })?)?;

            let merged: DS = calculator.merge_deltas(&base, &first, &second).await?;

            let mut patched = d2_value;
            patched["previousCheckpointId"] = serde_json::json!(anchor_id);
            patched["delta"] = serde_json::to_value(&merged)?;
            let updated: T = serde_json::from_value(patched)?;

            self.save(&updated, entity_type, entity_id).await?;
            self.delete(&d1.id).await?;

            chain.remove(0);
            if let Some(entry) = chain.first_mut() {
                entry.previous_checkpoint_id = Some(anchor_id.to_string());
                entry.chain_root_id = Some(anchor_id.to_string());
                entry.chain_position = Some(1);
            }
            let prev_ids: Vec<String> = chain.iter().map(|e| e.id.clone()).collect();
            for (index, entry) in chain.iter_mut().enumerate() {
                entry.chain_position = Some((index + 1) as u32);
                if index > 0 {
                    entry.previous_checkpoint_id = Some(prev_ids[index - 1].clone());
                    entry.chain_root_id = Some(anchor_id.to_string());
                }
            }
            merged_count += 1;
        }

        if merged_count > 0 {
            for index in 1..chain.len() {
                let current_id = chain[index].id.clone();
                let expected_previous = chain[index - 1].id.clone();
                let anchor = anchor_id
                    .as_deref()
                    .expect("anchor checked inside compaction loop");
                let needs_fix = chain[index].previous_checkpoint_id.as_deref()
                    != Some(expected_previous.as_str())
                    || chain[index].chain_root_id.as_deref() != Some(anchor)
                    || chain[index].chain_position != Some((index + 1) as u32);
                if !needs_fix {
                    continue;
                }
                let Some(value) = self.load(&current_id).await? else {
                    continue;
                };
                let mut patched = serde_json::to_value(&value)?;
                patched["previousCheckpointId"] = serde_json::json!(expected_previous);
                patched["chainRootId"] = serde_json::json!(anchor);
                patched["chainPosition"] = serde_json::json!((index + 1) as u32);
                let updated: T = serde_json::from_value(patched)?;
                self.save(&updated, entity_type, entity_id).await?;
                chain[index].previous_checkpoint_id = Some(expected_previous);
                chain[index].chain_root_id = Some(anchor.to_string());
                chain[index].chain_position = Some((index + 1) as u32);
            }
        }

        Ok(merged_count)
    }
}

impl<T> super::CheckpointStateManager for StorageBackedStateManager<T>
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
        let payload = payload_value(checkpoint)?;
        let id = field_as_str(&payload, "id")?;
        let checkpoint_type = checkpoint_type_of(&payload);
        let is_full = checkpoint_type == CheckpointType::Full;
        let timestamp = optional_i64_field(&payload, "timestamp")
            .or_else(|| self.clock.now_ms())
            .ok_or_else(|| {
                CheckpointError::Internal(
                    "checkpoint has no timestamp and the checkpoint clock is unavailable"
                        .to_string(),
                )
            })?;
        let base_checkpoint_id =
            optional_field_as_str(&payload, "baseCheckpointId", "base_checkpoint_id");
        let previous_checkpoint_id =
            optional_field_as_str(&payload, "previousCheckpointId", "previous_checkpoint_id");

        // compression is enabled on the save path with an `Auto`
        // strategy (payloads larger than the compression threshold are gzip
        // compressed; smaller payloads stay plain). Reads transparently
        // detect gzip via magic bytes, so the switch is format-compatible.
        // The async variant keeps the compressor's deflate working set off
        // the caller's stack (checkpoint saves sit deep inside nested
        // execution futures).
        let data = CheckpointSerializer::serialize_with_compression_async(
            checkpoint,
            CheckpointCodec::Json,
            CompressionStrategy::Auto,
        )
        .await?;

        let (chain_root_id, chain_position) = self
            .compute_chain_info(&id, &checkpoint_type, previous_checkpoint_id.as_deref())
            .await?;

        let metadata = self.build_metadata(MetadataArgs {
            id: &id,
            entity_type,
            entity_id,
            parent_entity_id: parent_entity_id_of(&payload).as_deref(),
            checkpoint_type,
            timestamp,
            base_checkpoint_id: base_checkpoint_id.as_deref(),
            previous_checkpoint_id: previous_checkpoint_id.as_deref(),
            chain_root_id: chain_root_id.as_deref(),
            chain_position,
            blob_size: data.len() as u64,
            compressed: CheckpointSerializer::is_compressed(&data),
            tags: tags_of(&payload).as_ref(),
            custom_fields: custom_fields_of(&payload).as_ref(),
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

/// Encode the checkpoint payload once for the metadata field reads on the save
/// path.
fn payload_value<T: Serialize>(checkpoint: &T) -> Result<Value, CheckpointError> {
    serde_json::to_value(checkpoint)
        .map_err(|e| CheckpointError::Serialization(format!("failed to serialize: {e}")))
}

fn field_as_str(payload: &Value, field: &str) -> Result<String, CheckpointError> {
    payload
        .get(field)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| CheckpointError::Validation {
            reason: format!("missing field: {field}"),
        })
}

/// A missing or unrecognised `type` field means a full checkpoint, which is
/// also what a payload written before type tracking looked like.
fn checkpoint_type_of(payload: &Value) -> CheckpointType {
    match payload.get("type").and_then(|v| v.as_str()) {
        Some("delta") | Some("DELTA") => CheckpointType::Delta,
        _ => CheckpointType::Full,
    }
}

fn optional_field_as_str(payload: &Value, field_camel: &str, field_snake: &str) -> Option<String> {
    payload
        .get(field_camel)
        .or_else(|| payload.get(field_snake))
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn optional_i64_field(payload: &Value, field: &str) -> Option<i64> {
    payload.get(field).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

fn tags_of(payload: &Value) -> Option<Vec<String>> {
    payload
        .get("metadata")
        .and_then(|m| m.get("tags"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn custom_fields_of(payload: &Value) -> Option<wf_types::Metadata> {
    payload
        .get("metadata")
        .and_then(|m| m.get("customFields").or_else(|| m.get("custom_fields")))
        .and_then(|v| v.as_object())
        .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// The parent execution id derived from the snapshot's materialised path.
/// Recorded on the storage metadata so child checkpoints are discoverable by
/// querying their parent, rather than by reading a child list the parent must
/// keep current. A delta has no snapshot, so it carries no link of its own and
/// `list_latest_by_parent` resolves the newest row per child entity instead.
fn parent_entity_id_of(payload: &Value) -> Option<String> {
    let path = payload
        .get("snapshot")
        .and_then(|s| s.get("hierarchy"))
        .and_then(|h| h.get("path"))
        .and_then(|v| v.as_str())?;

    let chain = wf_types::execution::decode_path(path);
    if chain.len() < 2 {
        return None;
    }
    chain.get(chain.len() - 2).cloned()
}

pub fn parse_storage_metadata(
    id: &str,
    entity_id: &str,
    meta: &Value,
) -> Result<CheckpointStorageMetadata, CheckpointError> {
    let entity_type = meta
        .get("entityType")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let cp_type = meta
        .get("checkpointType")
        .and_then(|v| v.as_str())
        .map(|s| match s.to_ascii_lowercase().as_str() {
            "delta" => CheckpointType::Delta,
            _ => CheckpointType::Full,
        })
        .unwrap_or(CheckpointType::Full);

    let Some(timestamp) = meta.get("timestamp").and_then(|v| v.as_i64()) else {
        return Err(CheckpointError::Corrupted {
            id: id.to_string(),
            reason: "checkpoint metadata missing timestamp".to_string(),
        });
    };
    let status = match meta.get("status").and_then(|v| v.as_str()) {
        None => wf_types::checkpoint::CheckpointStatus::Completed,
        Some(s) => {
            let normalized = s.to_ascii_lowercase();
            serde_json::from_str::<wf_types::checkpoint::CheckpointStatus>(&format!(
                "\"{}\"",
                normalized
            ))
            .map_err(|_| CheckpointError::Corrupted {
                id: id.to_string(),
                reason: format!("unknown checkpoint status '{s}'"),
            })?
        }
    };

    Ok(CheckpointStorageMetadata {
        id: id.to_string(),
        entity_type,
        entity_id: entity_id.to_string(),
        parent_entity_id: meta
            .get("parentEntityId")
            .and_then(|v| v.as_str())
            .map(String::from),
        checkpoint_type: cp_type,
        timestamp,
        status,
        previous_checkpoint_id: meta
            .get("previousCheckpointId")
            .and_then(|v| v.as_str())
            .map(String::from),
        base_checkpoint_id: meta
            .get("baseCheckpointId")
            .and_then(|v| v.as_str())
            .map(String::from),
        chain_root_id: meta
            .get("chainRootId")
            .and_then(|v| v.as_str())
            .map(String::from),
        chain_position: meta
            .get("chainPosition")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        blob_size: meta.get("blobSize").and_then(|v| v.as_u64()),
        tags: meta
            .get("tags")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        custom_fields: meta
            .get("customFields")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
    })
}

#[async_trait::async_trait]
impl<T: Send + Sync> CheckpointLoader for StorageBackedStateManager<T> {
    async fn load_checkpoint_data(&self, id: &str) -> Result<Option<Vec<u8>>, CheckpointError> {
        self.storage
            .load(id)
            .await
            .map(|entry| entry.map(|(data, _)| data))
            .map_err(CheckpointError::Storage)
    }

    async fn load_metadata(
        &self,
        id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        match self
            .storage
            .load(id)
            .await
            .map_err(CheckpointError::Storage)?
        {
            Some((_, meta)) => {
                let entity_id = meta
                    .get("entityId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                Ok(Some(parse_storage_metadata(id, &entity_id, &meta)?))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CheckpointStateManager;
    use serde_json::json;
    use std::sync::Arc;
    use wf_types::checkpoint::{BaseCheckpointCore, CheckpointType};

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct TestCheckpoint {
        id: String,
        checkpoint_type: Option<String>,
        entity_id: String,
        timestamp: i64,
        data: String,
    }

    fn make_storage() -> Arc<StorageBackend> {
        Arc::new(StorageBackend::new_memory())
    }

    type Envelope = BaseCheckpointCore<Value, Value>;

    fn make_envelope(
        id: &str,
        cp_type: Option<CheckpointType>,
        previous: Option<&str>,
        timestamp: i64,
        delta: Option<Value>,
        snapshot: Option<Value>,
    ) -> Envelope {
        BaseCheckpointCore {
            id: id.to_string(),
            r#type: cp_type,
            base_checkpoint_id: previous.map(String::from),
            previous_checkpoint_id: previous.map(String::from),
            delta,
            snapshot,
            timestamp: Some(timestamp),
            metadata: None,
            format_version: None,
        }
    }

    /// Trivial diff calculator where the delta carries the entire current
    /// state: diff(prev, curr) = curr, apply(base, delta) = delta.
    struct FullStateDiff;

    #[async_trait::async_trait]
    impl DiffCalculator<Value, Value> for FullStateDiff {
        async fn calculate_diff(
            &self,
            _previous: &Value,
            current: &Value,
        ) -> Result<Value, CheckpointError> {
            Ok(current.clone())
        }

        async fn apply_delta(
            &self,
            _base: &Value,
            delta: &Value,
        ) -> Result<Value, CheckpointError> {
            Ok(delta.clone())
        }
    }

    #[tokio::test]
    async fn save_and_load() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

        let cp = TestCheckpoint {
            id: "cp-1".to_string(),
            checkpoint_type: None,
            entity_id: "exec-1".to_string(),
            timestamp: 1000,
            data: "snapshot".to_string(),
        };

        mgr.save(&cp, "test", "exec-1").await.unwrap();
        let loaded = mgr.load("cp-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().data, "snapshot");
    }

    #[tokio::test]
    async fn load_tampered_payload_fails_and_marks_corrupted() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage.clone());

        let cp = TestCheckpoint {
            id: "cp-tamper".to_string(),
            checkpoint_type: None,
            entity_id: "exec-1".to_string(),
            timestamp: 1000,
            data: "snapshot-with-enough-bytes".to_string(),
        };
        mgr.save(&cp, "test", "exec-1").await.unwrap();

        // Flip a byte in the middle of the persisted payload without
        // touching the stored hash (simulates on-disk corruption).
        assert!(
            storage.corrupt_payload("cp-tamper", 12, 0xFF).await,
            "payload must be tampered"
        );

        let err = mgr.load("cp-tamper").await.unwrap_err();
        assert!(
            matches!(err, CheckpointError::Corrupted { .. }),
            "tampered payload must surface as Corrupted, got {err:?}"
        );

        // The metadata record is marked corrupted so recovery/listing can
        // see the checkpoint is unusable.
        let listed = storage.list(None).await.unwrap();
        let (_, meta) = listed
            .iter()
            .find(|(id, _)| id == "cp-tamper")
            .expect("tampered checkpoint metadata still listed");
        assert_eq!(
            meta.get("status").and_then(|v| v.as_str()),
            Some("corrupted"),
            "metadata status must be marked corrupted"
        );

        // Corrupted records are excluded from normal checkpoint loads.
        assert!(mgr.load("cp-tamper").await.is_err());
    }

    #[tokio::test]
    async fn load_missing() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);
        let loaded = mgr.load("nonexistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn delete_existing() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

        let cp = TestCheckpoint {
            id: "cp-1".to_string(),
            checkpoint_type: None,
            entity_id: "exec-1".to_string(),
            timestamp: 1000,
            data: "x".to_string(),
        };

        mgr.save(&cp, "test", "exec-1").await.unwrap();
        assert!(mgr.delete("cp-1").await.unwrap());
        assert!(!mgr.delete("cp-1").await.unwrap());
    }

    #[tokio::test]
    async fn list_by_entity_filters_correctly() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

        let cp1 = TestCheckpoint {
            id: "cp-1".to_string(),
            checkpoint_type: None,
            entity_id: "exec-1".to_string(),
            timestamp: 1000,
            data: "x".to_string(),
        };
        let cp2 = TestCheckpoint {
            id: "cp-2".to_string(),
            checkpoint_type: None,
            entity_id: "exec-2".to_string(),
            timestamp: 2000,
            data: "y".to_string(),
        };

        mgr.save(&cp1, "test", "exec-1").await.unwrap();
        mgr.save(&cp2, "test", "exec-2").await.unwrap();

        let list = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "cp-1");
    }

    #[tokio::test]
    async fn cleanup_removes_oldest() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

        for i in 0..5 {
            let cp = TestCheckpoint {
                id: format!("cp-{}", i),
                checkpoint_type: None,
                entity_id: "exec-1".to_string(),
                timestamp: i as i64 * 1000,
                data: format!("data-{}", i),
            };
            mgr.save(&cp, "test", "exec-1").await.unwrap();
        }

        let deleted = mgr.cleanup("exec-1", Some(2)).await.unwrap();
        assert_eq!(deleted, 3);

        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(remaining.len(), 2);
    }

    #[tokio::test]
    async fn cleanup_protects_delta_chain_members() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        mgr.save(
            &make_envelope(
                "full-1",
                None,
                None,
                1000,
                None,
                Some(json!({"state": "base"})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
        mgr.save(
            &make_envelope(
                "delta-1",
                Some(CheckpointType::Delta),
                Some("full-1"),
                2000,
                Some(json!({"state": "s1"})),
                None,
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
        mgr.save(
            &make_envelope(
                "delta-2",
                Some(CheckpointType::Delta),
                Some("delta-1"),
                3000,
                Some(json!({"state": "s2"})),
                None,
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
        mgr.save(
            &make_envelope(
                "delta-3",
                Some(CheckpointType::Delta),
                Some("delta-2"),
                4000,
                Some(json!({"state": "s3"})),
                None,
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();

        let deleted = mgr.cleanup("exec-1", Some(2)).await.unwrap();
        assert_eq!(deleted, 0);

        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(remaining.len(), 4);
    }

    #[tokio::test]
    async fn execute_cleanup_for_entity_respects_exclude_and_reports_bytes() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        for i in 0..4 {
            mgr.save(
                &make_envelope(
                    &format!("cp-{}", i),
                    None,
                    None,
                    1000 + i as i64,
                    None,
                    Some(json!({"state": i})),
                ),
                "test",
                "exec-1",
            )
            .await
            .unwrap();
        }

        let result = mgr
            .execute_cleanup_for_entity(
                "exec-1",
                "workflow_execution",
                Some("cp-0"),
                &CleanupStrategy::CountBased {
                    max_checkpoints: 1,
                    min_retention: 0,
                },
            )
            .await
            .unwrap();

        assert_eq!(result.deleted_count, 2);
        assert!(
            !result.deleted_checkpoint_ids.contains(&"cp-0".to_string()),
            "excluded checkpoint survives cleanup"
        );
        assert!(result.deleted_checkpoint_ids.contains(&"cp-1".to_string()));
        assert!(result.deleted_checkpoint_ids.contains(&"cp-2".to_string()));
        assert_eq!(result.remaining_count, 2);
        assert!(
            result.freed_bytes > 0,
            "freed bytes accounted from real blob sizes"
        );
    }

    #[tokio::test]
    async fn cleanup_uses_watermark_for_incremental_runs() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);
        let strategy = CleanupStrategy::CountBased {
            max_checkpoints: 1,
            min_retention: 0,
        };

        for i in 0..5 {
            mgr.save(
                &make_envelope(
                    &format!("cp-{}", i),
                    None,
                    None,
                    1000 + i as i64,
                    None,
                    Some(json!({"state": i})),
                ),
                "test",
                "exec-1",
            )
            .await
            .unwrap();
        }

        // First run is a full scan: only the newest checkpoint survives.
        let r1 = mgr
            .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
            .await
            .unwrap();
        assert_eq!(r1.deleted_count, 4);
        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "cp-4");

        // Newer checkpoints arrive after the watermark was persisted.
        for i in 5..7 {
            mgr.save(
                &make_envelope(
                    &format!("cp-{}", i),
                    None,
                    None,
                    5000 + (i - 5) as i64,
                    None,
                    Some(json!({"state": i})),
                ),
                "test",
                "exec-1",
            )
            .await
            .unwrap();
        }

        // Second run is incremental: only checkpoints newer than the
        // watermark are considered, so the old survivor is untouched.
        let r2 = mgr
            .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
            .await
            .unwrap();
        assert_eq!(r2.deleted_count, 1);
        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        let ids: Vec<&str> = remaining.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["cp-4", "cp-6"]);

        let (watermark, run_count) = mgr.load_entity_cleanup_metadata("exec-1").await.unwrap();
        assert_eq!(watermark, Some(5001));
        assert_eq!(run_count, 2);
    }

    #[tokio::test]
    async fn cleanup_watermark_clamps_future_and_advances_on_empty() {
        use checkpoint_base::clock::CheckpointClock;

        const T0: i64 = 1_000_000;
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage)
            .with_clock(CheckpointClock::manual(T0));
        let strategy = CleanupStrategy::CountBased {
            max_checkpoints: 10,
            min_retention: 0,
        };

        let future = T0 + 86_400_000;
        mgr.save(
            &make_envelope(
                "cp-future",
                None,
                None,
                future,
                None,
                Some(json!({"state": "future"})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();

        let result = mgr
            .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
            .await
            .unwrap();
        assert_eq!(result.deleted_count, 0);
        let (watermark, run_count) = mgr.load_entity_cleanup_metadata("exec-1").await.unwrap();
        // Deterministic clamp: the future survivor cannot lift the watermark
        // past the clock reading.
        assert_eq!(watermark, Some(T0));
        assert_eq!(run_count, 1);

        let second = mgr
            .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
            .await
            .unwrap();
        assert_eq!(second.deleted_count, 0);
        let (_, run_count) = mgr.load_entity_cleanup_metadata("exec-1").await.unwrap();
        assert_eq!(run_count, 2);
    }

    #[tokio::test]
    async fn cleanup_without_clock_refuses_watermark_write() {
        use checkpoint_base::clock::{CheckpointClock, ManualClock};

        let storage = make_storage();
        let clock = CheckpointClock::manual(1_000_000);
        let handle: ManualClock = clock.manual_handle().expect("manual clock");
        let mgr = StorageBackedStateManager::<Envelope>::new(storage).with_clock(clock);
        let strategy = CleanupStrategy::CountBased {
            max_checkpoints: 10,
            min_retention: 0,
        };
        mgr.save(
            &make_envelope("cp-1", None, None, 1000, None, Some(json!({"state": 1}))),
            "test",
            "exec-1",
        )
        .await
        .unwrap();

        handle.fail();
        let err = mgr
            .execute_cleanup_for_entity("exec-1", "test", None, &strategy)
            .await
            .expect_err("cleanup without a clock reading must fail closed");
        assert!(format!("{err:?}").contains("clock unavailable"));
    }

    #[tokio::test]
    async fn concurrent_cleanup_serialized_per_entity() {
        let storage = make_storage();
        let mgr = Arc::new(StorageBackedStateManager::<Envelope>::new(storage));

        for i in 0..8 {
            mgr.save(
                &make_envelope(
                    &format!("cp-{}", i),
                    None,
                    None,
                    1000 + i as i64,
                    None,
                    Some(json!({"state": i})),
                ),
                "test",
                "exec-1",
            )
            .await
            .unwrap();
        }

        let mut handles = Vec::new();
        for _ in 0..4 {
            let mgr = mgr.clone();
            handles.push(tokio::spawn(async move {
                let _result = mgr
                    .execute_cleanup_for_entity(
                        "exec-1",
                        "test",
                        None,
                        &CleanupStrategy::CountBased {
                            max_checkpoints: 2,
                            min_retention: 1,
                        },
                    )
                    .await
                    .unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(remaining.len(), 2, "cleanup converges to the limit");
    }

    #[tokio::test]
    async fn cleanup_with_strategy_respects_cleanup_strategy() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        for i in 0..5 {
            mgr.save(
                &make_envelope(
                    &format!("cp-{}", i),
                    None,
                    None,
                    i as i64 * 1000,
                    None,
                    Some(json!({"state": i})),
                ),
                "test",
                "exec-1",
            )
            .await
            .unwrap();
        }

        let deleted = mgr
            .cleanup_with_strategy(
                "exec-1",
                &CleanupStrategy::CountBased {
                    max_checkpoints: 2,
                    min_retention: 1,
                },
            )
            .await
            .unwrap();
        assert_eq!(deleted, 3);

        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(remaining.len(), 2);

        // New checkpoints created after the persisted watermark.
        for i in 5..7 {
            mgr.save(
                &make_envelope(
                    &format!("cp-{}", i),
                    None,
                    None,
                    5000 + (i - 5) as i64,
                    None,
                    Some(json!({"state": i})),
                ),
                "test",
                "exec-1",
            )
            .await
            .unwrap();
        }

        // Time-based strategy removes everything older than the window;
        // the latest checkpoint is always protected from deletion.
        let deleted = mgr
            .cleanup_with_strategy(
                "exec-1",
                &CleanupStrategy::TimeBased {
                    max_age_seconds: 86_400,
                    min_retention: 1,
                },
            )
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        let remaining = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(remaining.len(), 3);
    }

    #[tokio::test]
    async fn list_latest_by_entities_returns_newest_per_entity() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        // Multiple checkpoints per entity, interleaved timestamps.
        for (i, entity) in [(0, "exec-1"), (0, "exec-2"), (1, "exec-1")] {
            mgr.save(
                &make_envelope(
                    &format!("cp-{}-{}", entity, i),
                    None,
                    None,
                    1000 + i as i64,
                    None,
                    Some(json!({"state": i})),
                ),
                "test",
                entity,
            )
            .await
            .unwrap();
        }
        // Unrelated entity must not leak into the IN query.
        mgr.save(
            &make_envelope(
                "cp-other-0",
                None,
                None,
                9000,
                None,
                Some(json!({"state": "x"})),
            ),
            "test",
            "exec-3",
        )
        .await
        .unwrap();

        let latest = mgr
            .list_latest_by_entities(&["exec-1".to_string(), "exec-2".to_string()])
            .await
            .unwrap();

        assert_eq!(latest.len(), 2);
        let by_entity: std::collections::HashMap<_, _> = latest
            .into_iter()
            .map(|m| (m.entity_id.clone(), m.id.clone()))
            .collect();
        assert_eq!(
            by_entity.get("exec-1").map(String::as_str),
            Some("cp-exec-1-1")
        );
        assert_eq!(
            by_entity.get("exec-2").map(String::as_str),
            Some("cp-exec-2-0")
        );
    }

    #[tokio::test]
    async fn list_latest_by_parent_returns_a_child_whose_newest_row_is_a_delta() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);
        let parent = json!({"hierarchy": {"path": "/parent-1/child-/"}});

        mgr.save(
            &make_envelope("child-full", None, None, 1000, None, Some(parent.clone())),
            "workflow_execution",
            "child-1",
        )
        .await
        .unwrap();
        mgr.save(
            &make_envelope(
                "child-delta",
                Some(CheckpointType::Delta),
                Some("child-full"),
                2000,
                Some(json!({"state": "s1"})),
                None,
            ),
            "workflow_execution",
            "child-1",
        )
        .await
        .unwrap();
        // A second child whose newest row is a full checkpoint.
        mgr.save(
            &make_envelope("other-full", None, None, 1500, None, Some(parent)),
            "workflow_execution",
            "child-2",
        )
        .await
        .unwrap();

        let children = mgr.list_latest_by_parent("parent-1").await.unwrap();
        let by_entity: std::collections::HashMap<_, _> = children
            .into_iter()
            .map(|m| (m.entity_id.clone(), m.id.clone()))
            .collect();
        assert_eq!(by_entity.len(), 2);
        assert_eq!(
            by_entity.get("child-1").map(String::as_str),
            Some("child-delta"),
            "the newest row is a delta, which carries no snapshot and so no parent link"
        );
        assert_eq!(
            by_entity.get("child-2").map(String::as_str),
            Some("other-full")
        );
    }

    #[tokio::test]
    async fn list_latest_by_parent_ignores_checkpoints_of_other_parents() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        mgr.save(
            &make_envelope(
                "mine",
                None,
                None,
                1000,
                None,
                Some(json!({"hierarchy": {"path": "/parent-1/child-1/"}})),
            ),
            "workflow_execution",
            "child-1",
        )
        .await
        .unwrap();
        mgr.save(
            &make_envelope(
                "theirs",
                None,
                None,
                2000,
                None,
                Some(json!({"hierarchy": {"path": "/parent-2/child-2/"}})),
            ),
            "workflow_execution",
            "child-2",
        )
        .await
        .unwrap();

        let children = mgr.list_latest_by_parent("parent-1").await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].entity_id, "child-1");
    }

    #[tokio::test]
    async fn metadata_chain_info_round_trip() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        mgr.save(
            &make_envelope(
                "full-1",
                None,
                None,
                1000,
                None,
                Some(json!({"state": "base"})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
        mgr.save(
            &make_envelope(
                "delta-1",
                Some(CheckpointType::Delta),
                Some("full-1"),
                2000,
                Some(json!({"state": "s1"})),
                None,
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();

        let all = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(all.len(), 2);

        let full_meta = &all[0];
        assert_eq!(full_meta.chain_root_id, Some("full-1".to_string()));
        assert_eq!(full_meta.chain_position, Some(0));
        assert!(full_meta.blob_size.unwrap_or(0) > 0);

        let delta_meta = &all[1];
        assert_eq!(delta_meta.chain_root_id, Some("full-1".to_string()));
        assert_eq!(delta_meta.chain_position, Some(1));
        assert!(delta_meta.blob_size.unwrap_or(0) > 0);
    }

    #[tokio::test]
    async fn load_batch_reports_missing() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        mgr.save(
            &make_envelope("cp-1", None, None, 1000, None, Some(json!({"state": "a"}))),
            "test",
            "exec-1",
        )
        .await
        .unwrap();

        let loaded = mgr
            .load_batch(&["cp-1".to_string(), "missing".to_string()])
            .await
            .unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].as_ref().expect("cp-1 present").id, "cp-1");
        assert!(loaded[1].is_none());
    }

    #[tokio::test]
    async fn metrics_recorded_on_save_and_load() {
        let storage = make_storage();
        let metrics = Arc::new(CheckpointMetricsCollector::new(
            wf_metrics::CollectorConfig::default(),
        ));
        let mgr = StorageBackedStateManager::<Envelope>::new(storage).with_metrics(metrics.clone());

        mgr.save(
            &make_envelope("cp-1", None, None, 1000, None, Some(json!({"state": "a"}))),
            "test",
            "exec-1",
        )
        .await
        .unwrap();

        let _ = mgr.load("cp-1").await.unwrap();
        let _ = mgr.load("missing").await.unwrap();

        let stats = metrics.usage_stats();
        assert_eq!(stats.creation_count, 1);
        assert_eq!(stats.load_count, 2);
        assert_eq!(stats.load_failures, 1);
    }

    #[tokio::test]
    async fn compact_delta_chain_merges_and_fixes_successor() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        mgr.save(
            &make_envelope(
                "full-1",
                None,
                None,
                1000,
                None,
                Some(json!({"state": "base"})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
        mgr.save(
            &make_envelope(
                "delta-1",
                Some(CheckpointType::Delta),
                Some("full-1"),
                2000,
                Some(json!({"state": "mid"})),
                None,
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
        mgr.save(
            &make_envelope(
                "delta-2",
                Some(CheckpointType::Delta),
                Some("delta-1"),
                3000,
                Some(json!({"state": "final"})),
                None,
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();

        let merged = mgr
            .compact_delta_chain("exec-1", "test", &FullStateDiff, 1)
            .await
            .unwrap();
        assert_eq!(merged, 1);

        assert!(mgr.load("delta-1").await.unwrap().is_none());

        let successor = mgr.load("delta-2").await.unwrap().unwrap();
        assert_eq!(successor.previous_checkpoint_id, Some("full-1".to_string()));
        assert_eq!(successor.delta, Some(json!({"state": "final"})));

        let successor_meta = CheckpointLoader::load_metadata(&mgr, "delta-2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(successor_meta.chain_root_id, Some("full-1".to_string()));
        assert_eq!(successor_meta.chain_position, Some(1));

        let restored = FullStateDiff
            .apply_delta(&json!({"state": "base"}), successor.delta.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(restored, json!({"state": "final"}));
    }

    #[tokio::test]
    async fn compact_delta_chain_merges_multiple_pairs() {
        let storage = make_storage();
        let mgr = StorageBackedStateManager::<Envelope>::new(storage);

        mgr.save(
            &make_envelope(
                "full-1",
                None,
                None,
                1000,
                None,
                Some(json!({"state": "0"})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
        for i in 1..=4 {
            let id = format!("delta-{}", i);
            let prev = if i == 1 {
                "full-1".to_string()
            } else {
                format!("delta-{}", i - 1)
            };
            mgr.save(
                &make_envelope(
                    &id,
                    Some(CheckpointType::Delta),
                    Some(&prev),
                    (1000 + i * 100) as i64,
                    Some(json!({"state": i})),
                    None,
                ),
                "test",
                "exec-1",
            )
            .await
            .unwrap();
        }

        let merged = mgr
            .compact_delta_chain("exec-1", "test", &FullStateDiff, 2)
            .await
            .unwrap();
        assert_eq!(merged, 2);

        let all = mgr.list_by_entity("exec-1").await.unwrap();
        assert_eq!(all.len(), 3);

        let last = mgr.load("delta-4").await.unwrap().unwrap();
        assert_eq!(last.delta, Some(json!({"state": 4})));
        assert_eq!(last.previous_checkpoint_id, Some("delta-3".to_string()));

        let restored = FullStateDiff
            .apply_delta(&json!({"state": 0}), last.delta.as_ref().unwrap())
            .await
            .unwrap();
        assert_eq!(restored, json!({"state": 4}));
    }

    #[test]
    fn parse_storage_metadata_accepts_status_case_variants() {
        let meta = json!({
            "entityType": "test",
            "checkpointType": "DELTA",
            "timestamp": 1000,
            "status": "CORRUPTED",
        });
        let parsed = parse_storage_metadata("cp-1", "exec-1", &meta).unwrap();
        assert_eq!(
            parsed.status,
            wf_types::checkpoint::CheckpointStatus::Corrupted
        );
        assert_eq!(parsed.checkpoint_type, CheckpointType::Delta);

        let mixed = json!({
            "entityType": "test",
            "checkpointType": "delta",
            "timestamp": 1000,
            "status": "Completed",
        });
        let parsed = parse_storage_metadata("cp-2", "exec-1", &mixed).unwrap();
        assert_eq!(
            parsed.status,
            wf_types::checkpoint::CheckpointStatus::Completed
        );
    }

    #[test]
    fn parse_storage_metadata_rejects_missing_timestamp_and_unknown_status() {
        let missing = json!({
            "entityType": "test",
            "checkpointType": "full",
            "status": "completed",
        });
        assert!(parse_storage_metadata("cp-x", "exec-1", &missing).is_err());

        let unknown = json!({
            "entityType": "test",
            "checkpointType": "full",
            "timestamp": 1000,
            "status": "bogus-status",
        });
        assert!(parse_storage_metadata("cp-y", "exec-1", &unknown).is_err());

        let absent_status_defaults_to_completed = json!({
            "entityType": "test",
            "checkpointType": "full",
            "timestamp": 1000,
        });
        let parsed =
            parse_storage_metadata("cp-z", "exec-1", &absent_status_defaults_to_completed).unwrap();
        assert_eq!(
            parsed.status,
            wf_types::checkpoint::CheckpointStatus::Completed
        );
    }
}

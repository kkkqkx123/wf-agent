use crate::cleanup::{CleanupExecutor, CleanupResult, CleanupStrategy};
use crate::delta::{CheckpointLoader, DiffCalculator};
use crate::error::CheckpointError;
use crate::metrics::CheckpointMetricsCollector;
use crate::serializer::{CheckpointCodec, CheckpointSerializer};
use crate::state::CheckpointStateManager;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use wf_storage::backend::StorageBackend;
use wf_storage::domain::store::{QueryFilter, Store};
use wf_types::checkpoint::CheckpointType;
use wf_types::storage::CheckpointStorageMetadata;

/// Reserved record key prefix for per-entity cleanup metadata (watermark).
/// The record's metadata carries no `entityId`, so it never matches
/// `list_by_entity` filters.
const ENTITY_CLEANUP_META_KEY_PREFIX: &str = "__checkpoint_cleanup_meta__:";

/// Every Nth cleanup run is a full scan (aligned with TS `cleanupRunCount % 10`).
const FULL_SCAN_INTERVAL: u64 = 10;

pub struct StorageBackedStateManager<T> {
    storage: Arc<StorageBackend>,
    metrics: Option<Arc<CheckpointMetricsCollector>>,
    /// Per-entity cleanup mutexes so concurrent cleanup runs for the same
    /// entity are serialized (aligned with the TS `cleanupLocks`).
    cleanup_locks: dashmap::DashMap<String, Arc<tokio::sync::Mutex<()>>>,
    _marker: std::marker::PhantomData<T>,
}

impl<T> StorageBackedStateManager<T> {
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self {
            storage,
            metrics: None,
            cleanup_locks: dashmap::DashMap::new(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn with_metrics(mut self, metrics: Arc<CheckpointMetricsCollector>) -> Self {
        self.metrics = Some(metrics);
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
        match self.storage.load(&key).await.map_err(CheckpointError::Storage)? {
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

    /// Persist the cleanup watermark for an entity (aligned with TS
    /// `setEntityMetadata(entityId, { cleanupWatermark, cleanupRunCount })`).
    async fn save_entity_cleanup_metadata(
        &self,
        entity_id: &str,
        watermark: i64,
        run_count: u64,
    ) -> Result<(), CheckpointError> {
        let key = Self::entity_cleanup_meta_key(entity_id);
        let metadata = serde_json::json!({
            "cleanupWatermark": watermark,
            "cleanupRunCount": run_count,
        });
        self.storage
            .save(&key, &[], &metadata)
            .await
            .map_err(CheckpointError::Storage)
    }

    fn build_metadata(&self, args: MetadataArgs<'_>) -> Value {
        serde_json::json!({
            "id": args.id,
            "entityType": args.entity_type,
            "entityId": args.entity_id,
            "checkpointType": args.checkpoint_type,
            "timestamp": args.timestamp,
            "status": "completed",
            "baseCheckpointId": args.base_checkpoint_id,
            "previousCheckpointId": args.previous_checkpoint_id,
            "chainRootId": args.chain_root_id,
            "chainPosition": args.chain_position,
            "blobSize": args.blob_size,
            "tags": args.tags,
            "customFields": args.custom_fields,
        })
    }
}

/// Aggregated fields for building a checkpoint storage metadata document.
struct MetadataArgs<'a> {
    id: &'a str,
    entity_type: &'a str,
    entity_id: &'a str,
    checkpoint_type: CheckpointType,
    timestamp: i64,
    base_checkpoint_id: Option<&'a str>,
    previous_checkpoint_id: Option<&'a str>,
    chain_root_id: Option<&'a str>,
    chain_position: Option<u32>,
    blob_size: u64,
    tags: Option<&'a Vec<String>>,
    custom_fields: Option<&'a serde_json::Map<String, Value>>,
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
                Some(prev) => match self.load_metadata(prev).await? {
                    Some(meta) => Ok((
                        meta.chain_root_id.or_else(|| Some(prev.to_string())),
                        Some(meta.chain_position.unwrap_or(0) + 1),
                    )),
                    None => Ok((Some(id.to_string()), Some(0))),
                },
                None => Ok((Some(id.to_string()), Some(0))),
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
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut latest: Vec<CheckpointStorageMetadata> = Vec::new();
        for (id, meta) in entries {
            let entity_id = meta
                .get("entityId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if seen.insert(entity_id.clone()) {
                latest.push(parse_storage_metadata(&id, &entity_id, &meta));
            }
        }
        Ok(latest)
    }

    fn extract_tags(&self, checkpoint: &T) -> Option<Vec<String>> {
        serde_json::to_value(checkpoint).ok().and_then(|json| {
            json.get("metadata")
                .and_then(|m| m.get("tags"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
        })
    }

    fn extract_custom_fields(&self, checkpoint: &T) -> Option<serde_json::Map<String, Value>> {
        serde_json::to_value(checkpoint).ok().and_then(|json| {
            json.get("metadata")
                .and_then(|m| {
                    m.get("customFields").or_else(|| m.get("custom_fields"))
                })
                .and_then(|v| v.as_object())
                .cloned()
        })
    }

    fn extract_json_count(checkpoint: &T, field: &str) -> u32 {
        serde_json::to_value(checkpoint)
            .ok()
            .and_then(|json| {
                json.get(field)
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.len() as u32)
            })
            .unwrap_or(0)
    }

    fn extract_variable_count(checkpoint: &T) -> u32 {
        serde_json::to_value(checkpoint)
            .ok()
            .and_then(|json| {
                json.get("variable_state")
                    .and_then(|v| v.get("variables"))
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.len() as u32)
            })
            .unwrap_or(0)
    }

    /// Execute a cleanup run for an entity with dependency protection,
    /// aligned with the TS `BaseCheckpointStateManager.executeCleanupForEntity`:
    /// per-entity cleanup serialization, optional excluded checkpoint id,
    /// real `blob_size`-based freed byte accounting, and a `CleanupResult`.
    ///
    /// Incremental semantics (TS watermark): every 10th run is a full scan;
    /// otherwise only checkpoints newer than the persisted watermark (plus
    /// the excluded id) are considered candidates. After a run the watermark
    /// advances to the newest remaining checkpoint timestamp.
    pub async fn execute_cleanup_for_entity(
        &self,
        entity_id: &str,
        _entity_type: &str,
        exclude_checkpoint_id: Option<&str>,
        strategy: &CleanupStrategy,
    ) -> Result<CleanupResult, CheckpointError> {
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
        let candidates: Vec<CheckpointStorageMetadata> =
            match last_watermark {
                Some(watermark) if !is_full_scan => all
                    .iter()
                    .filter(|c| {
                        c.timestamp > watermark
                            || Some(c.id.as_str()) == exclude_checkpoint_id
                    })
                    .cloned()
                    .collect(),
                _ => all.clone(),
            };

        let executor = CleanupExecutor::new();
        let mut result = executor.evaluate_protected_with_result(&candidates, strategy);

        if let Some(exclude) = exclude_checkpoint_id {
            result
                .deleted_checkpoint_ids
                .retain(|id| id != exclude);
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
        let mut deleted = 0u64;
        for id in &result.deleted_checkpoint_ids {
            if self.delete(id).await? {
                deleted += 1;
            }
        }
        result.deleted_count = deleted;
        result.remaining_count = candidates.len().saturating_sub(deleted as usize) as u64;

        if let Some(ref metrics) = self.metrics {
            metrics.record_cleanup(&wf_types::checkpoint::CheckpointCleanupMetrics {
                deleted_count: deleted as u32,
                freed_bytes: result.freed_bytes,
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }

        // Advance the watermark to the newest surviving checkpoint so the next
        // incremental run only looks at checkpoints created after this run.
        let survivors: Vec<&CheckpointStorageMetadata> = candidates
            .iter()
            .filter(|c| !result.deleted_checkpoint_ids.contains(&c.id))
            .collect();
        if let Some(max_timestamp) = survivors.iter().map(|c| c.timestamp).max() {
            self.save_entity_cleanup_metadata(entity_id, max_timestamp, run_count + 1)
                .await?;
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
                Some(prev) => self.load_metadata(prev).await?,
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
            merged_count += 1;
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
        let id = extract_field_as_str(checkpoint, "id")?;
        let checkpoint_type = extract_checkpoint_type(checkpoint)?;
        let is_full = checkpoint_type == CheckpointType::Full;
        let timestamp = extract_optional_i64_field(checkpoint, "timestamp")?
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        let base_checkpoint_id =
            extract_optional_field_as_str(checkpoint, "baseCheckpointId", "base_checkpoint_id")?;
        let previous_checkpoint_id = extract_optional_field_as_str(
            checkpoint,
            "previousCheckpointId",
            "previous_checkpoint_id",
        )?;

        let data = CheckpointSerializer::serialize(checkpoint, CheckpointCodec::Json)?;

        let (chain_root_id, chain_position) = self
            .compute_chain_info(&id, &checkpoint_type, previous_checkpoint_id.as_deref())
            .await?;
        let tags = self.extract_tags(checkpoint);
        let custom_fields = self.extract_custom_fields(checkpoint);

        let metadata = self.build_metadata(MetadataArgs {
            id: &id,
            entity_type,
            entity_id,
            checkpoint_type,
            timestamp,
            base_checkpoint_id: base_checkpoint_id.as_deref(),
            previous_checkpoint_id: previous_checkpoint_id.as_deref(),
            chain_root_id: chain_root_id.as_deref(),
            chain_position,
            blob_size: data.len() as u64,
            tags: tags.as_ref(),
            custom_fields: custom_fields.as_ref(),
        });

        self.storage
            .save(&id, &data, &metadata)
            .await
            .map_err(CheckpointError::Storage)?;

        if let Some(ref metrics) = self.metrics {
            metrics.record_creation_for_entity(
                entity_id,
                &wf_types::checkpoint::CheckpointCreationMetrics {
                    duration_ms: start.elapsed().as_millis() as u64,
                    size_bytes: data.len() as u64,
                    node_count: Self::extract_json_count(checkpoint, "node_results"),
                    variable_count: Self::extract_variable_count(checkpoint),
                },
                is_full,
            );
            metrics.record_chain_length(&wf_types::checkpoint::CheckpointChainLengthMetric {
                entity_id: entity_id.to_string(),
                chain_length: chain_position.unwrap_or(0) + 1,
                delta_count: if is_full { 0 } else { chain_position.unwrap_or(0) },
            });
        }

        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<Self::Checkpoint>, CheckpointError> {
        let start = Instant::now();
        match self
            .storage
            .load(id)
            .await
            .map_err(CheckpointError::Storage)?
        {
            Some((data, _)) => {
                let size = data.len() as u64;
                let checkpoint = CheckpointSerializer::auto_deserialize(&data)?;
                if let Some(ref metrics) = self.metrics {
                    metrics.record_load(
                        &wf_types::checkpoint::CheckpointLoadMetrics {
                            duration_ms: start.elapsed().as_millis() as u64,
                            size_bytes: size,
                            compressed: false,
                        },
                        true,
                    );
                }
                Ok(Some(checkpoint))
            }
            None => {
                if let Some(ref metrics) = self.metrics {
                    metrics.record_load(
                        &wf_types::checkpoint::CheckpointLoadMetrics {
                            duration_ms: start.elapsed().as_millis() as u64,
                            size_bytes: 0,
                            compressed: false,
                        },
                        false,
                    );
                }
                Ok(None)
            }
        }
    }

    async fn load_batch(&self, ids: &[String]) -> Result<Vec<Self::Checkpoint>, CheckpointError> {
        let mut result = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(checkpoint) = self.load(id).await? {
                result.push(checkpoint);
            }
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

        let mut results: Vec<CheckpointStorageMetadata> = entries
            .into_iter()
            .map(|(id, meta)| parse_storage_metadata(&id, entity_id, &meta))
            .collect();

        results.sort_by_key(|m| m.timestamp);
        Ok(results)
    }

    async fn get_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<CheckpointStorageMetadata>, CheckpointError> {
        let mut all = self.list_by_entity(entity_id).await?;
        Ok(all.pop())
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
        let result = self
            .execute_cleanup_for_entity(entity_id, "unknown", None, strategy)
            .await?;
        Ok(result.deleted_count)
    }
}

fn extract_optional_field_as_str<T: Serialize>(
    value: &T,
    field_camel: &str,
    field_snake: &str,
) -> Result<Option<String>, CheckpointError> {
    let json = serde_json::to_value(value).map_err(|e| {
        CheckpointError::Serialization(format!(
            "failed to serialize for field {}: {}",
            field_camel, e
        ))
    })?;
    Ok(json
        .get(field_camel)
        .or_else(|| json.get(field_snake))
        .and_then(|v| v.as_str())
        .map(String::from))
}

fn extract_optional_i64_field<T: Serialize>(
    value: &T,
    field: &str,
) -> Result<Option<i64>, CheckpointError> {
    let json = serde_json::to_value(value).map_err(|e| {
        CheckpointError::Serialization(format!("failed to serialize for field {}: {}", field, e))
    })?;
    Ok(json
        .get(field)
        .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))))
}

fn parse_storage_metadata(id: &str, entity_id: &str, meta: &Value) -> CheckpointStorageMetadata {
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

    let timestamp = meta.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
    let status = meta
        .get("status")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            serde_json::from_str::<wf_types::checkpoint::CheckpointStatus>(&format!("\"{}\"", s))
                .ok()
        })
        .unwrap_or(wf_types::checkpoint::CheckpointStatus::Active);

    CheckpointStorageMetadata {
        id: id.to_string(),
        entity_type,
        entity_id: entity_id.to_string(),
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
    }
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
                Ok(Some(parse_storage_metadata(id, &entity_id, &meta)))
            }
            None => Ok(None),
        }
    }
}

fn extract_field_as_str<T: Serialize>(value: &T, field: &str) -> Result<String, CheckpointError> {
    let json = serde_json::to_value(value).map_err(|e| {
        CheckpointError::Serialization(format!("failed to serialize for field {}: {}", field, e))
    })?;
    json.get(field)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| CheckpointError::Validation {
            reason: format!("missing field: {}", field),
        })
}

fn extract_checkpoint_type<T: Serialize>(value: &T) -> Result<CheckpointType, CheckpointError> {
    let json = serde_json::to_value(value).map_err(|e| {
        CheckpointError::Serialization(format!("failed to serialize for type extraction: {}", e))
    })?;
    match json.get("type").and_then(|v| v.as_str()) {
        Some("delta") | Some("DELTA") => Ok(CheckpointType::Delta),
        _ => Ok(CheckpointType::Full),
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
        assert_eq!(by_entity.get("exec-1").map(String::as_str), Some("cp-exec-1-1"));
        assert_eq!(by_entity.get("exec-2").map(String::as_str), Some("cp-exec-2-0"));
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
    async fn load_batch_skips_missing() {
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
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "cp-1");
    }

    #[tokio::test]
    async fn metrics_recorded_on_save_and_load() {
        let storage = make_storage();
        let metrics = Arc::new(CheckpointMetricsCollector::new());
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

        let agg = metrics.aggregate();
        assert_eq!(agg.total_checkpoints, 1);
        assert_eq!(agg.full_checkpoints, 1);
        assert_eq!(agg.load_count, 2);
        assert_eq!(agg.load_success, 1);
        assert_eq!(agg.load_failed, 1);
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

        let successor_meta = mgr.load_metadata("delta-2").await.unwrap().unwrap();
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
}

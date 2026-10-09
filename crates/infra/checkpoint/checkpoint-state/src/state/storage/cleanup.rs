//! Per-entity cleanup with a persisted watermark.
//!
//! Cleanup runs are serialized per entity and evaluated by the shared cleanup
//! policy executor. Every tenth run is a full scan; in between only rows
//! newer than the persisted watermark are candidates. Deletes and the
//! watermark advance land in one atomic batch so a crash between the two
//! can never skip surviving checkpoints in the next incremental run.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use checkpoint_base::cleanup_policy::{CleanupExecutor, CleanupResult, CleanupStrategy};
use checkpoint_base::error::CheckpointError;
use wf_storage::domain::store::{BatchItem, Store, StoreExt, StoreOperation};
use wf_types::storage::CheckpointStorageMetadata;

use crate::state::CheckpointStateManager;

/// Reserved record key prefix for per-entity cleanup metadata (watermark).
/// The record's metadata carries no `entityId`, so it never matches
/// `list_by_entity` filters.
const ENTITY_CLEANUP_META_KEY_PREFIX: &str = "__checkpoint_cleanup_meta__:";

/// Every Nth cleanup run is a full scan.
const FULL_SCAN_INTERVAL: u64 = 10;

impl<T> super::StorageBackedStateManager<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    pub(super) fn entity_cleanup_meta_key(entity_id: &str) -> String {
        format!("{ENTITY_CLEANUP_META_KEY_PREFIX}{entity_id}")
    }

    /// Load the persisted cleanup watermark for an entity.
    /// Returns `(last_watermark, run_count)`.
    pub(super) async fn load_entity_cleanup_metadata(
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
    pub(super) fn entity_cleanup_metadata_item(
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
}

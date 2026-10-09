//! Indexed metadata reads.
//!
//! Every read is expressed as a storage query (filters pushed down to the
//! backend) and parsed row by row; corrupt rows are skipped loudly so one bad
//! row never fails the batch.

use std::collections::HashSet;

use checkpoint_base::error::CheckpointError;
use wf_storage::domain::store::{QueryFilter, Store};
use wf_types::storage::CheckpointStorageMetadata;

use super::metadata::parse_storage_metadata;

impl<T> super::StorageBackedStateManager<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
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
        let mut seen: HashSet<String> = HashSet::new();
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
}

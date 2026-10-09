//! Delta chain derivation and compaction.
//!
//! A checkpoint row records which chain root it belongs to and its position
//! inside that chain; compaction merges the oldest consecutive delta pairs so
//! a long-running execution never carries an unbounded chain.

use serde::Serialize;

use checkpoint_base::delta::{CheckpointLoader, DiffCalculator};
use checkpoint_base::error::CheckpointError;
use wf_types::checkpoint::CheckpointType;
use wf_types::storage::CheckpointStorageMetadata;

use crate::state::CheckpointStateManager;

impl<T> super::StorageBackedStateManager<T>
where
    T: Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    /// Derive the chain root and position of a checkpoint being written. A
    /// delta must link to a readable predecessor: falling back to a new chain
    /// head would silently hide a broken chain, so the missing link is
    /// reported loudly.
    pub(super) async fn compute_chain_info(
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

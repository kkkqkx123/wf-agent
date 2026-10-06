use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::super::checkpoint::CheckpointStatus;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointStorageMetadata {
    pub id: super::super::Id,
    pub entity_type: String,
    pub entity_id: String,
    /// Execution this checkpoint's entity was spawned from, when it has a
    /// parent. Carried as a forward link so "which checkpoints belong to the
    /// children of X" is an indexed lookup instead of a manifest the parent
    /// has to keep up to date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_entity_id: Option<String>,
    pub checkpoint_type: super::super::checkpoint::CheckpointType,
    pub timestamp: super::super::Timestamp,
    pub status: CheckpointStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_checkpoint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_checkpoint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_root_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<super::super::Metadata>,
}

impl CheckpointStorageMetadata {
    /// The indexed metadata document a checkpoint row is stored under. Every
    /// writer of a checkpoint row builds it here so the key set has exactly
    /// one definition; `compressed` is left out because it describes the
    /// encoded blob rather than the record, and each writer knows it.
    pub fn metadata_document(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("id".into(), json!(self.id));
        map.insert("entityType".into(), json!(self.entity_type));
        map.insert("entityId".into(), json!(self.entity_id));
        if let Some(parent) = &self.parent_entity_id {
            map.insert("parentEntityId".into(), json!(parent));
        }
        map.insert("checkpointType".into(), json!(self.checkpoint_type));
        map.insert("timestamp".into(), json!(self.timestamp));
        map.insert("status".into(), json!(self.status));
        if let Some(previous) = &self.previous_checkpoint_id {
            map.insert("previousCheckpointId".into(), json!(previous));
        }
        if let Some(base) = &self.base_checkpoint_id {
            map.insert("baseCheckpointId".into(), json!(base));
        }
        if let Some(root) = &self.chain_root_id {
            map.insert("chainRootId".into(), json!(root));
        }
        if let Some(position) = self.chain_position {
            map.insert("chainPosition".into(), json!(position));
        }
        if let Some(size) = self.blob_size {
            map.insert("blobSize".into(), json!(size));
        }
        if let Some(tags) = &self.tags {
            map.insert("tags".into(), json!(tags));
        }
        if let Some(custom_fields) = &self.custom_fields {
            map.insert("customFields".into(), json!(custom_fields));
        }
        Value::Object(map)
    }
}

pub type Checkpoint = CheckpointStorageMetadata;

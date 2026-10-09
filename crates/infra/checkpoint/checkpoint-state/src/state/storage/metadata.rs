//! Indexed metadata document construction and payload field reads.
//!
//! The save path reads every indexed field off one serialization of the
//! payload and turns it into one storage document, so the field readers here
//! must tolerate both camelCase (wire) and snake_case (in-memory) shapes.

use serde::Serialize;
use serde_json::Value;

use checkpoint_base::error::CheckpointError;
use wf_types::checkpoint::CheckpointType;
use wf_types::storage::CheckpointStorageMetadata;

/// Aggregated fields for building a checkpoint storage metadata document.
pub(super) struct MetadataArgs<'a> {
    pub(super) id: &'a str,
    pub(super) entity_type: &'a str,
    pub(super) entity_id: &'a str,
    pub(super) parent_entity_id: Option<&'a str>,
    pub(super) checkpoint_type: CheckpointType,
    pub(super) timestamp: i64,
    pub(super) base_checkpoint_id: Option<&'a str>,
    pub(super) previous_checkpoint_id: Option<&'a str>,
    pub(super) chain_root_id: Option<&'a str>,
    pub(super) chain_position: Option<u32>,
    pub(super) blob_size: u64,
    pub(super) compressed: bool,
    pub(super) tags: Option<&'a Vec<String>>,
    pub(super) custom_fields: Option<&'a wf_types::Metadata>,
}

impl<T> super::StorageBackedStateManager<T> {
    /// Build the indexed metadata document for one checkpoint row. The record
    /// type owns the key set; `compressed` describes the encoded blob, which
    /// only this writer knows.
    pub(super) fn build_metadata(&self, args: MetadataArgs<'_>) -> Value {
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

/// Encode the checkpoint payload once for the metadata field reads on the save
/// path.
pub(super) fn payload_value<T: Serialize>(checkpoint: &T) -> Result<Value, CheckpointError> {
    serde_json::to_value(checkpoint)
        .map_err(|e| CheckpointError::Serialization(format!("failed to serialize: {e}")))
}

pub(super) fn field_as_str(payload: &Value, field: &str) -> Result<String, CheckpointError> {
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
pub(super) fn checkpoint_type_of(payload: &Value) -> CheckpointType {
    match payload.get("type").and_then(|v| v.as_str()) {
        Some("delta") | Some("DELTA") => CheckpointType::Delta,
        _ => CheckpointType::Full,
    }
}

pub(super) fn optional_field_as_str(
    payload: &Value,
    field_camel: &str,
    field_snake: &str,
) -> Option<String> {
    payload
        .get(field_camel)
        .or_else(|| payload.get(field_snake))
        .and_then(|v| v.as_str())
        .map(String::from)
}

pub(super) fn optional_i64_field(payload: &Value, field: &str) -> Option<i64> {
    payload.get(field).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

pub(super) fn tags_of(payload: &Value) -> Option<Vec<String>> {
    payload
        .get("metadata")
        .and_then(|m| m.get("tags"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

pub(super) fn custom_fields_of(payload: &Value) -> Option<wf_types::Metadata> {
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
pub(super) fn parent_entity_id_of(payload: &Value) -> Option<String> {
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

/// Parse one storage document into indexed checkpoint metadata. Corrupt rows
/// (missing timestamp, unknown status) surface as `Corrupted` so listings can
/// skip them loudly instead of misreporting state.
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

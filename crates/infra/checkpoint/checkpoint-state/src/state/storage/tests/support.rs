//! Shared fixtures for storage-backed state manager tests.

pub(crate) use crate::state::storage::metadata::parse_storage_metadata;
pub(crate) use crate::state::storage::StorageBackedStateManager;
use checkpoint_base::delta::DiffCalculator;
use checkpoint_base::error::CheckpointError;
use serde_json::Value;
use std::sync::Arc;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::{BaseCheckpointCore, CheckpointType};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct TestCheckpoint {
    pub(crate) id: String,
    pub(crate) checkpoint_type: Option<String>,
    pub(crate) entity_id: String,
    pub(crate) timestamp: i64,
    pub(crate) data: String,
}

pub(crate) fn make_storage() -> Arc<StorageBackend> {
    Arc::new(StorageBackend::new_memory())
}

pub(crate) type Envelope = BaseCheckpointCore<Value, Value>;

pub(crate) fn make_envelope(
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
pub(crate) struct FullStateDiff;

#[async_trait::async_trait]
impl DiffCalculator<Value, Value> for FullStateDiff {
    async fn calculate_diff(
        &self,
        _previous: &Value,
        current: &Value,
    ) -> Result<Value, CheckpointError> {
        Ok(current.clone())
    }

    async fn apply_delta(&self, _base: &Value, delta: &Value) -> Result<Value, CheckpointError> {
        Ok(delta.clone())
    }
}

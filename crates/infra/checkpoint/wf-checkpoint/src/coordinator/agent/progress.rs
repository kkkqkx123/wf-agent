use checkpoint_base::metadata::builder::{
    CHAIN_POSITION_FIELD, ITERATION_FIELD, LOOP_STATUS_FIELD, MSG_SEQ_END_FIELD,
    MSG_SEQ_NEXT_FIELD, MSG_SEQ_START_FIELD, PENDING_COUNT_FIELD, TOOL_CALL_COUNT_FIELD,
};
use std::collections::HashMap;
use wf_types::checkpoint::agent::AgentStateSnapshot;
use wf_types::storage::CheckpointStorageMetadata;

/// Progress coordinates of a checkpoint: conversation sequence bounds plus
/// loop progress counters plus the in-flight tool call count, read from
/// metadata without loading blobs. Equal coordinates mean no side effect
/// landed since the recorded checkpoint. Rows predating the coordinate
/// fields read as `None` and never compare equal to a fresh build, so dedup
/// over old history is fail-open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressCoords {
    pub seq_start: Option<u64>,
    pub seq_end: Option<u64>,
    pub seq_next: Option<u64>,
    pub iteration: Option<u64>,
    pub tool_call_count: Option<u64>,
    pub loop_status: Option<String>,
    pub pending_count: Option<u64>,
}

impl ProgressCoords {
    /// Render coordinates as stored custom fields for the shared gate
    /// comparison over
    /// [`checkpoint_base::metadata::builder::PROGRESS_COORD_KEYS`].
    /// Absent values render as JSON null, mirroring what `build` injects, so
    /// a fresh build compares equal to its own persisted row while
    /// pre-coordinate rows never match.
    pub fn as_fields(&self) -> HashMap<String, serde_json::Value> {
        HashMap::from([
            (
                MSG_SEQ_START_FIELD.to_string(),
                serde_json::json!(self.seq_start),
            ),
            (
                MSG_SEQ_END_FIELD.to_string(),
                serde_json::json!(self.seq_end),
            ),
            (
                MSG_SEQ_NEXT_FIELD.to_string(),
                serde_json::json!(self.seq_next),
            ),
            (
                ITERATION_FIELD.to_string(),
                serde_json::json!(self.iteration),
            ),
            (
                TOOL_CALL_COUNT_FIELD.to_string(),
                serde_json::json!(self.tool_call_count),
            ),
            (
                LOOP_STATUS_FIELD.to_string(),
                serde_json::json!(self.loop_status),
            ),
            (
                PENDING_COUNT_FIELD.to_string(),
                serde_json::json!(self.pending_count),
            ),
        ])
    }
}

/// Coordinates plus the chain position, rendered as the caller custom
/// fields `build` stamps on the row.
pub(super) fn progress_custom_fields(
    coords: &ProgressCoords,
    chain_position: u32,
) -> HashMap<String, serde_json::Value> {
    let mut fields = coords.as_fields();
    fields.insert(
        CHAIN_POSITION_FIELD.to_string(),
        serde_json::json!(chain_position),
    );
    fields
}

/// Progress coordinates from stored checkpoint metadata.
pub fn progress_coords(meta: &CheckpointStorageMetadata) -> ProgressCoords {
    let get = |key: &str| {
        meta.custom_fields
            .as_ref()
            .and_then(|fields| fields.get(key))
    };
    ProgressCoords {
        seq_start: get(MSG_SEQ_START_FIELD).and_then(|v| v.as_u64()),
        seq_end: get(MSG_SEQ_END_FIELD).and_then(|v| v.as_u64()),
        seq_next: get(MSG_SEQ_NEXT_FIELD).and_then(|v| v.as_u64()),
        iteration: get(ITERATION_FIELD).and_then(|v| v.as_u64()),
        tool_call_count: get(TOOL_CALL_COUNT_FIELD).and_then(|v| v.as_u64()),
        loop_status: get(LOOP_STATUS_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        pending_count: get(PENDING_COUNT_FIELD).and_then(|v| v.as_u64()),
    }
}

/// Progress coordinates of a not-yet-persisted snapshot. Mirrors the
/// coordinate fields `build` injects, so a seed or live snapshot can be
/// compared against stored metadata before any blob is written.
pub fn snapshot_progress_coords(snapshot: &AgentStateSnapshot) -> ProgressCoords {
    ProgressCoords {
        seq_start: snapshot.message_seq_start,
        seq_end: snapshot.message_seq_end,
        seq_next: snapshot.message_next_seq,
        iteration: Some(snapshot.current_iteration as u64),
        tool_call_count: Some(snapshot.tool_call_count as u64),
        loop_status: Some(snapshot.status.clone()),
        pending_count: Some(pending_call_count(snapshot)),
    }
}

/// Number of in-flight tool calls carried by a snapshot. Stored as a plain
/// count so crash recovery can tell a flight window apart from idle.
fn pending_call_count(snapshot: &AgentStateSnapshot) -> u64 {
    snapshot
        .pending_tool_call_ids
        .as_ref()
        .map(|ids| ids.len() as u64)
        .unwrap_or(0)
}

use checkpoint_base::metadata::builder::{
    fingerprint_entries, fingerprint_option, CHAIN_POSITION_FIELD, WF_CURRENT_NODE_FIELD,
    WF_NODE_RESULTS_HASH_FIELD, WF_RECORD_COUNT_FIELD, WF_STATUS_FIELD,
    WF_TRIGGER_STATES_HASH_FIELD, WF_VARIABLES_HASH_FIELD,
};
use std::collections::{BTreeMap, HashMap};
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::storage::CheckpointStorageMetadata;

/// Progress coordinates of a workflow checkpoint: execution status, resume
/// pointer, content hashes of node results and variables, the audit record
/// count and the trigger-state hash — read from metadata without loading
/// blobs. Equal coordinates mean no side effect landed since the recorded
/// checkpoint. Rows predating the coordinate fields read as `None` and never
/// compare equal to a fresh build, so dedup over old history is fail-open.
///
/// The resume pointer (`current_node_id`) is deliberately included: the row
/// after node A and the row before node B differ only in it, and their
/// restore paths differ (completed-node skip with successor re-derivation
/// versus direct continuation), so they are not duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowProgressCoords {
    pub status: Option<String>,
    pub current_node: Option<String>,
    pub node_results_hash: Option<String>,
    pub variables_hash: Option<String>,
    pub record_count: Option<u64>,
    pub trigger_states_hash: Option<String>,
}

impl WorkflowProgressCoords {
    /// Render coordinates as stored custom fields (`None` as JSON null,
    /// mirroring what `build` injects) for the shared gate comparison over
    /// [`checkpoint_base::metadata::builder::WF_PROGRESS_COORD_KEYS`].
    pub fn as_fields(&self) -> HashMap<String, serde_json::Value> {
        HashMap::from([
            (WF_STATUS_FIELD.to_string(), serde_json::json!(self.status)),
            (
                WF_CURRENT_NODE_FIELD.to_string(),
                serde_json::json!(self.current_node),
            ),
            (
                WF_NODE_RESULTS_HASH_FIELD.to_string(),
                serde_json::json!(self.node_results_hash),
            ),
            (
                WF_VARIABLES_HASH_FIELD.to_string(),
                serde_json::json!(self.variables_hash),
            ),
            (
                WF_RECORD_COUNT_FIELD.to_string(),
                serde_json::json!(self.record_count),
            ),
            (
                WF_TRIGGER_STATES_HASH_FIELD.to_string(),
                serde_json::json!(self.trigger_states_hash),
            ),
        ])
    }
}

/// Coordinates plus the chain position, rendered as the caller custom
/// fields `build` stamps on the row.
pub(super) fn progress_custom_fields(
    coords: &WorkflowProgressCoords,
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
pub fn workflow_progress_coords(meta: &CheckpointStorageMetadata) -> WorkflowProgressCoords {
    let get = |key: &str| {
        meta.custom_fields
            .as_ref()
            .and_then(|fields| fields.get(key))
    };
    WorkflowProgressCoords {
        status: get(WF_STATUS_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        current_node: get(WF_CURRENT_NODE_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        node_results_hash: get(WF_NODE_RESULTS_HASH_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        variables_hash: get(WF_VARIABLES_HASH_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
        record_count: get(WF_RECORD_COUNT_FIELD).and_then(|v| v.as_u64()),
        trigger_states_hash: get(WF_TRIGGER_STATES_HASH_FIELD)
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

/// Hash one string-keyed value map with sorted keys so map iteration order
/// never affects the fingerprint. Same-key value changes alter the hash, so
/// counter-style variable overwrites still force a new row.
fn hash_value_map(map: &HashMap<String, serde_json::Value>) -> String {
    let entries: BTreeMap<String, Vec<u8>> = map
        .iter()
        .map(|(key, value)| (key.clone(), serde_json::to_vec(value).unwrap_or_default()))
        .collect();
    fingerprint_entries(&entries)
}

/// Progress coordinates of a not-yet-persisted snapshot. Mirrors the
/// coordinate fields `build` injects, so a live snapshot can be compared
/// against stored metadata before any blob is written. Computed from the
/// pre-policy snapshot: content filtering may strip blob domains, but the
/// coordinates describe the execution state, not the stored payload.
pub fn snapshot_workflow_coords(
    snapshot: &WorkflowExecutionStateSnapshot,
) -> WorkflowProgressCoords {
    let empty: HashMap<String, serde_json::Value> = HashMap::new();
    WorkflowProgressCoords {
        status: Some(snapshot.status.clone()),
        current_node: snapshot.current_node_id.clone(),
        node_results_hash: Some(hash_value_map(
            snapshot.node_results.as_ref().unwrap_or(&empty),
        )),
        variables_hash: Some(hash_value_map(&snapshot.variable_state.variables)),
        record_count: Some(
            snapshot
                .node_execution_records
                .as_ref()
                .map(Vec::len)
                .unwrap_or(0) as u64,
        ),
        trigger_states_hash: Some(fingerprint_option(&snapshot.trigger_states)),
    }
}

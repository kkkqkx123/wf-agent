use std::collections::BTreeMap;
use std::collections::HashMap;
use wf_types::checkpoint::base::{CheckpointMetadata, CheckpointStateBase};
use wf_types::checkpoint::CheckpointTiming;
use wf_types::Id;

/// Custom field keys injected into checkpoint metadata (`formatVersion`,
/// `createdAt`) and the storage chain metadata (`chainPosition`).
pub const FORMAT_VERSION_FIELD: &str = "formatVersion";
pub const CREATED_AT_FIELD: &str = "createdAt";
pub const CHAIN_POSITION_FIELD: &str = "chainPosition";

/// Conversation sequence bounds recorded on every checkpoint so progress
/// can be read from metadata without loading blobs.
pub const MSG_SEQ_START_FIELD: &str = "msgSeqStart";
pub const MSG_SEQ_END_FIELD: &str = "msgSeqEnd";
pub const MSG_SEQ_NEXT_FIELD: &str = "msgNextSeq";

/// Loop progress counters recorded on every agent checkpoint. Together with
/// the sequence bounds they form the progress coordinates: equal coordinates
/// mean no side effect landed since the recorded checkpoint, so a repeat
/// creation can merge back into it instead of persisting a duplicate row.
pub const ITERATION_FIELD: &str = "iteration";
pub const TOOL_CALL_COUNT_FIELD: &str = "toolCallCount";
pub const LOOP_STATUS_FIELD: &str = "loopStatus";

/// In-flight tool call count recorded on every agent checkpoint. Committed
/// progress alone cannot see a tool flight window, so a checkpoint landing
/// there with no other change would be skipped and the latest row would miss
/// the in-flight record. The count is a single number, never identifiers;
/// stream buffers stay excluded as pure transients.
pub const PENDING_COUNT_FIELD: &str = "pendingToolCallCount";

/// Workflow progress coordinate fields recorded on every workflow
/// checkpoint. Equal coordinates mean the execution produced no side effect
/// since the recorded checkpoint, so a repeat creation can merge back into
/// it instead of persisting a duplicate row. Scalar signals stay readable;
/// map-valued domains are stored as stable hashes (hex FNV-1a over sorted
/// key/bytes entries) so same-key value changes still force a new row
/// without duplicating blob contents into metadata.
pub const WF_STATUS_FIELD: &str = "workflowStatus";
pub const WF_CURRENT_NODE_FIELD: &str = "workflowCurrentNode";
pub const WF_NODE_RESULTS_HASH_FIELD: &str = "workflowNodeResultsHash";
pub const WF_VARIABLES_HASH_FIELD: &str = "workflowVariablesHash";
pub const WF_RECORD_COUNT_FIELD: &str = "workflowNodeRecordCount";
pub const WF_TRIGGER_STATES_HASH_FIELD: &str = "workflowTriggerStatesHash";

/// Deterministic 64-bit FNV-1a over the bytes, hex-encoded. Unlike the
/// default hasher this is stable across processes, so metadata written by
/// one process compares equal when read by another.
fn stable_hash_hex(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

/// Fingerprint a string map: keys are already sorted (`BTreeMap`), each
/// entry is length-prefixed so key/value boundary shifts cannot collide.
pub fn fingerprint_entries(entries: &BTreeMap<String, Vec<u8>>) -> String {
    let mut hash_bytes = Vec::new();
    for (key, value) in entries {
        hash_bytes.extend_from_slice(&(key.len() as u64).to_le_bytes());
        hash_bytes.extend_from_slice(key.as_bytes());
        hash_bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        hash_bytes.extend_from_slice(value);
    }
    stable_hash_hex(&hash_bytes)
}

/// Fingerprint one JSON value by its canonical bytes. `None` serializes as
/// `null`, so an absent domain still yields a stable (non-missing) hash —
/// callers keep the Option layer to distinguish pre-coordinate rows.
pub fn fingerprint_option(value: &Option<serde_json::Value>) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    stable_hash_hex(&bytes)
}

/// Custom-field keys forming the agent progress coordinates, in one list so
/// the shared gate compares exactly the injected set.
pub const PROGRESS_COORD_KEYS: &[&str] = &[
    MSG_SEQ_START_FIELD,
    MSG_SEQ_END_FIELD,
    MSG_SEQ_NEXT_FIELD,
    ITERATION_FIELD,
    TOOL_CALL_COUNT_FIELD,
    LOOP_STATUS_FIELD,
    PENDING_COUNT_FIELD,
];

/// Custom-field keys forming the workflow progress coordinates, in one list
/// so the shared gate compares exactly the injected set.
pub const WF_PROGRESS_COORD_KEYS: &[&str] = &[
    WF_STATUS_FIELD,
    WF_CURRENT_NODE_FIELD,
    WF_NODE_RESULTS_HASH_FIELD,
    WF_VARIABLES_HASH_FIELD,
    WF_RECORD_COUNT_FIELD,
    WF_TRIGGER_STATES_HASH_FIELD,
];

/// Compare stored custom fields against freshly computed ones over an
/// explicit key list. Shared by the agent-loop and workflow progress gates
/// so both resolve "no side effect since latest" identically. Missing on
/// both sides counts as equal; missing on one side does not, so rows
/// predating the coordinate fields never match a fresh build. Callers render
/// absent values as JSON null (mirroring what `build` injects) rather than
/// omitting keys, so a fresh build compares equal to its own persisted row.
pub fn custom_fields_equal(
    stored: &Option<HashMap<String, serde_json::Value>>,
    current: &HashMap<String, serde_json::Value>,
    keys: &[&str],
) -> bool {
    let empty = HashMap::new();
    let stored = stored.as_ref().unwrap_or(&empty);
    keys.iter().all(|key| stored.get(*key) == current.get(*key))
}

#[derive(Debug, Clone)]
pub struct CheckpointMetadataBuilder {
    description: Option<String>,
    tags: Vec<String>,
    custom_fields: Option<HashMap<String, serde_json::Value>>,
}

impl CheckpointMetadataBuilder {
    pub fn new() -> Self {
        Self {
            description: None,
            tags: Vec::new(),
            custom_fields: None,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn custom_field(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        let map = self.custom_fields.get_or_insert_with(HashMap::new);
        map.insert(key.into(), value);
        self
    }

    pub fn build(self) -> CheckpointMetadata {
        CheckpointMetadata {
            description: self.description,
            tags: if self.tags.is_empty() {
                None
            } else {
                Some(self.tags)
            },
            custom_fields: self.custom_fields,
        }
    }
}

impl Default for CheckpointMetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Human-readable description for a checkpoint trigger, used as the
/// checkpoint metadata `description` (trigger-based descriptions such as
/// "Before node" / "Error checkpoint").
pub fn trigger_description(trigger: &CheckpointTiming) -> String {
    match trigger {
        CheckpointTiming::BeforeExecute => "Before execute",
        CheckpointTiming::AfterExecute => "After execute",
        CheckpointTiming::OnError => "Error checkpoint",
        CheckpointTiming::BeforeRetry => "Before retry",
        CheckpointTiming::AfterRetrySuccess => "After retry success",
        CheckpointTiming::OnFallback => "Fallback checkpoint",
        CheckpointTiming::IterationEnd => "Iteration end",
        CheckpointTiming::IterationFailed => "Iteration failed",
        CheckpointTiming::ToolBefore => "Before tool",
        CheckpointTiming::ToolAfter => "After tool",
        CheckpointTiming::BeforeCompression => "Before compression",
        CheckpointTiming::AfterCompression => "After compression",
        CheckpointTiming::OnPause => "Pause checkpoint",
        CheckpointTiming::OnCancel => "Cancel checkpoint",
        CheckpointTiming::OnTimeout => "Timeout checkpoint",
        CheckpointTiming::OnFailure => "Failure checkpoint",
        CheckpointTiming::OnStopped => "Stopped checkpoint",
        CheckpointTiming::OnComplete => "Complete checkpoint",
        CheckpointTiming::Interval => "Interval checkpoint",
        CheckpointTiming::Manual => "Manual checkpoint",
        CheckpointTiming::Never => "Never",
    }
    .to_string()
}

/// The wire name of a trigger, e.g. `"BEFORE_EXECUTE"` — used for the
/// `trigger:<name>` metadata tag.
pub fn trigger_tag(trigger: &CheckpointTiming) -> String {
    format!("trigger:{}", trigger_wire_name(trigger))
}

fn trigger_wire_name(trigger: &CheckpointTiming) -> &'static str {
    match trigger {
        CheckpointTiming::BeforeExecute => "BEFORE_EXECUTE",
        CheckpointTiming::AfterExecute => "AFTER_EXECUTE",
        CheckpointTiming::OnError => "ON_ERROR",
        CheckpointTiming::BeforeRetry => "BEFORE_RETRY",
        CheckpointTiming::AfterRetrySuccess => "AFTER_RETRY_SUCCESS",
        CheckpointTiming::OnFallback => "ON_FALLBACK",
        CheckpointTiming::IterationEnd => "ITERATION_END",
        CheckpointTiming::IterationFailed => "ITERATION_FAILED",
        CheckpointTiming::ToolBefore => "TOOL_BEFORE",
        CheckpointTiming::ToolAfter => "TOOL_AFTER",
        CheckpointTiming::BeforeCompression => "BEFORE_COMPRESSION",
        CheckpointTiming::AfterCompression => "AFTER_COMPRESSION",
        CheckpointTiming::OnPause => "ON_PAUSE",
        CheckpointTiming::OnCancel => "ON_CANCEL",
        CheckpointTiming::OnTimeout => "ON_TIMEOUT",
        CheckpointTiming::OnFailure => "ON_FAILURE",
        CheckpointTiming::OnStopped => "ON_STOPPED",
        CheckpointTiming::OnComplete => "ON_COMPLETE",
        CheckpointTiming::Interval => "INTERVAL",
        CheckpointTiming::Manual => "MANUAL",
        CheckpointTiming::Never => "NEVER",
    }
}

/// Build the checkpoint metadata object: the wire format is a flat map with
/// the keys `description` / `tags` / `customFields` (`CheckpointMetadata`
/// shape). Caller custom fields are merged with the injected `formatVersion`
/// and `createdAt` fields (injected values win).
/// Returns `None` only when there is no content at all.
pub fn build_checkpoint_metadata(
    description: Option<String>,
    tags: Vec<String>,
    custom_fields: HashMap<String, serde_json::Value>,
    format_version: &str,
) -> Option<HashMap<String, serde_json::Value>> {
    let has_content = description.is_some() || !tags.is_empty() || !custom_fields.is_empty();
    if !has_content {
        return None;
    }

    let mut merged = custom_fields;
    merged.insert(
        FORMAT_VERSION_FIELD.to_string(),
        serde_json::json!(format_version),
    );
    merged.insert(
        CREATED_AT_FIELD.to_string(),
        serde_json::json!(chrono::Utc::now().timestamp_millis()),
    );

    let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
    if let Some(description) = description {
        metadata.insert("description".to_string(), serde_json::json!(description));
    }
    if !tags.is_empty() {
        metadata.insert("tags".to_string(), serde_json::json!(tags));
    }
    metadata.insert(
        "customFields".to_string(),
        serde_json::Value::Object(merged.into_iter().collect()),
    );
    Some(metadata)
}

pub fn build_checkpoint_state(
    id: Id,
    workflow_id: Option<Id>,
    execution_id: Option<Id>,
) -> CheckpointStateBase {
    CheckpointStateBase {
        id,
        workflow_id,
        execution_id,
        timestamp: wf_common::time::now(),
        format_version: "1.0".to_string(),
        status: Some("active".to_string()),
        start_time: None,
        end_time: None,
        error: None,
        error_records: None,
        interruption_records: None,
        event_records: None,
        hierarchy: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_builder() {
        let metadata = CheckpointMetadataBuilder::new()
            .description("test checkpoint")
            .tag("auto")
            .tag("node-1")
            .custom_field("node_id", serde_json::json!("node-1"))
            .build();

        assert_eq!(metadata.description, Some("test checkpoint".to_string()));
        assert_eq!(
            metadata.tags,
            Some(vec!["auto".to_string(), "node-1".to_string()])
        );
        assert!(metadata.custom_fields.is_some());
    }

    #[test]
    fn test_build_checkpoint_state() {
        let state = build_checkpoint_state(Id::new(), None, None);
        assert_eq!(state.format_version, "1.0");
        assert_eq!(state.status, Some("active".to_string()));
    }

    #[test]
    fn trigger_description_maps_all_triggers() {
        assert_eq!(
            trigger_description(&CheckpointTiming::OnError),
            "Error checkpoint"
        );
        assert_eq!(
            trigger_description(&CheckpointTiming::AfterExecute),
            "After execute"
        );
        assert_eq!(
            trigger_description(&CheckpointTiming::Manual),
            "Manual checkpoint"
        );
    }

    #[test]
    fn trigger_tag_uses_wire_name() {
        assert_eq!(
            trigger_tag(&CheckpointTiming::BeforeExecute),
            "trigger:BEFORE_EXECUTE"
        );
        assert_eq!(trigger_tag(&CheckpointTiming::OnPause), "trigger:ON_PAUSE");
        assert_eq!(
            trigger_tag(&CheckpointTiming::OnFailure),
            "trigger:ON_FAILURE"
        );
        assert_eq!(
            trigger_tag(&CheckpointTiming::OnStopped),
            "trigger:ON_STOPPED"
        );
    }

    #[test]
    fn terminal_triggers_stay_distinguishable() {
        // A failed run must not read as an in-flight error checkpoint, and a
        // stopped run must not read as a cancel: both pairs used to share one
        // trigger and were indistinguishable on restore.
        assert_ne!(
            trigger_tag(&CheckpointTiming::OnFailure),
            trigger_tag(&CheckpointTiming::OnError)
        );
        assert_ne!(
            trigger_tag(&CheckpointTiming::OnStopped),
            trigger_tag(&CheckpointTiming::OnCancel)
        );
        assert_eq!(
            trigger_description(&CheckpointTiming::OnFailure),
            "Failure checkpoint"
        );
        assert_eq!(
            trigger_description(&CheckpointTiming::OnStopped),
            "Stopped checkpoint"
        );
    }

    #[test]
    fn build_checkpoint_metadata_merges_custom_fields() {
        let mut fields = HashMap::new();
        fields.insert("node_id".to_string(), serde_json::json!("node-1"));
        fields.insert(FORMAT_VERSION_FIELD.to_string(), serde_json::json!("stale"));
        let metadata = build_checkpoint_metadata(
            Some("desc".to_string()),
            vec!["trigger:MANUAL".to_string()],
            fields,
            "1.1.0",
        )
        .unwrap();
        assert_eq!(
            metadata.get("description").and_then(|v| v.as_str()),
            Some("desc")
        );
        assert_eq!(
            metadata.get("tags"),
            Some(&serde_json::json!(["trigger:MANUAL"]))
        );
        let custom = metadata.get("customFields").unwrap().as_object().unwrap();
        assert_eq!(
            custom.get(FORMAT_VERSION_FIELD),
            Some(&serde_json::json!("1.1.0"))
        );
        assert!(custom.get(CREATED_AT_FIELD).is_some());
        assert_eq!(custom.get("node_id"), Some(&serde_json::json!("node-1")));
    }

    #[test]
    fn build_checkpoint_metadata_none_without_content() {
        assert!(build_checkpoint_metadata(None, vec![], HashMap::new(), "1.1.0").is_none());
    }

    #[test]
    fn fingerprint_entries_stable_across_insertion_order() {
        let first: BTreeMap<String, Vec<u8>> = [
            ("b".to_string(), b"2".to_vec()),
            ("a".to_string(), b"1".to_vec()),
        ]
        .into_iter()
        .collect();
        let second: BTreeMap<String, Vec<u8>> = [
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"2".to_vec()),
        ]
        .into_iter()
        .collect();
        assert_eq!(fingerprint_entries(&first), fingerprint_entries(&second));
        let changed: BTreeMap<String, Vec<u8>> = [
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"3".to_vec()),
        ]
        .into_iter()
        .collect();
        assert_ne!(fingerprint_entries(&first), fingerprint_entries(&changed));
        assert_ne!(
            fingerprint_option(&None),
            fingerprint_option(&Some(serde_json::json!({"a": 1})))
        );
    }

    #[test]
    fn custom_fields_equal_needs_both_sides_present() {
        let stored = Some(HashMap::from([
            ("a".to_string(), serde_json::json!(1)),
            ("b".to_string(), serde_json::Value::Null),
        ]));
        let current = HashMap::from([
            ("a".to_string(), serde_json::json!(1)),
            ("b".to_string(), serde_json::Value::Null),
        ]);
        assert!(custom_fields_equal(&stored, &current, &["a", "b"]));
        // Missing on one side never matches, so pre-coordinate rows stay
        // fail-open; missing on both sides is equal.
        assert!(!custom_fields_equal(&None, &current, &["a"]));
        assert!(custom_fields_equal(&None, &HashMap::new(), &["a"]));
        let changed = HashMap::from([
            ("a".to_string(), serde_json::json!(2)),
            ("b".to_string(), serde_json::Value::Null),
        ]);
        assert!(!custom_fields_equal(&stored, &changed, &["a", "b"]));
    }
}

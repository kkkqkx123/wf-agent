use std::collections::HashMap;
use wf_types::checkpoint::base::{CheckpointMetadata, CheckpointStateBase};
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::Id;

/// Custom field keys injected into checkpoint metadata, aligned with the TS
/// `buildCheckpointMetadata` (`formatVersion`, `createdAt`) and the storage
/// chain metadata (`chainPosition`).
pub const FORMAT_VERSION_FIELD: &str = "formatVersion";
pub const CREATED_AT_FIELD: &str = "createdAt";
pub const CHAIN_POSITION_FIELD: &str = "chainPosition";

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
/// checkpoint metadata `description` (aligned with the TS trigger-based
/// descriptions such as "Before node" / "Error checkpoint").
pub fn trigger_description(trigger: &CheckpointTrigger) -> String {
    match trigger {
        CheckpointTrigger::BeforeExecute => "Before execute",
        CheckpointTrigger::AfterExecute => "After execute",
        CheckpointTrigger::OnError => "Error checkpoint",
        CheckpointTrigger::BeforeRetry => "Before retry",
        CheckpointTrigger::AfterRetrySuccess => "After retry success",
        CheckpointTrigger::OnFallback => "Fallback checkpoint",
        CheckpointTrigger::IterationEnd => "Iteration end",
        CheckpointTrigger::IterationFailed => "Iteration failed",
        CheckpointTrigger::ToolBefore => "Before tool",
        CheckpointTrigger::ToolAfter => "After tool",
        CheckpointTrigger::OnPause => "Pause checkpoint",
        CheckpointTrigger::OnCancel => "Cancel checkpoint",
        CheckpointTrigger::OnComplete => "Complete checkpoint",
        CheckpointTrigger::Interval => "Interval checkpoint",
        CheckpointTrigger::Manual => "Manual checkpoint",
        CheckpointTrigger::Never => "Never",
    }
    .to_string()
}

/// The wire name of a trigger, e.g. `"BEFORE_EXECUTE"` — used for the
/// `trigger:<name>` metadata tag.
pub fn trigger_tag(trigger: &CheckpointTrigger) -> String {
    format!("trigger:{}", trigger_wire_name(trigger))
}

fn trigger_wire_name(trigger: &CheckpointTrigger) -> &'static str {
    match trigger {
        CheckpointTrigger::BeforeExecute => "BEFORE_EXECUTE",
        CheckpointTrigger::AfterExecute => "AFTER_EXECUTE",
        CheckpointTrigger::OnError => "ON_ERROR",
        CheckpointTrigger::BeforeRetry => "BEFORE_RETRY",
        CheckpointTrigger::AfterRetrySuccess => "AFTER_RETRY_SUCCESS",
        CheckpointTrigger::OnFallback => "ON_FALLBACK",
        CheckpointTrigger::IterationEnd => "ITERATION_END",
        CheckpointTrigger::IterationFailed => "ITERATION_FAILED",
        CheckpointTrigger::ToolBefore => "TOOL_BEFORE",
        CheckpointTrigger::ToolAfter => "TOOL_AFTER",
        CheckpointTrigger::OnPause => "ON_PAUSE",
        CheckpointTrigger::OnCancel => "ON_CANCEL",
        CheckpointTrigger::OnComplete => "ON_COMPLETE",
        CheckpointTrigger::Interval => "INTERVAL",
        CheckpointTrigger::Manual => "MANUAL",
        CheckpointTrigger::Never => "NEVER",
    }
}

/// Build the checkpoint metadata object aligned with the TS
/// `buildCheckpointMetadata`: the wire format is a flat map with the keys
/// `description` / `tags` / `customFields` (TS `CheckpointMetadata` shape).
/// Caller custom fields are merged with the injected `formatVersion` and
/// `createdAt` fields (injected values win, matching TS merge semantics).
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
        assert_eq!(trigger_description(&CheckpointTrigger::OnError), "Error checkpoint");
        assert_eq!(trigger_description(&CheckpointTrigger::AfterExecute), "After execute");
        assert_eq!(trigger_description(&CheckpointTrigger::Manual), "Manual checkpoint");
    }

    #[test]
    fn trigger_tag_uses_wire_name() {
        assert_eq!(trigger_tag(&CheckpointTrigger::BeforeExecute), "trigger:BEFORE_EXECUTE");
        assert_eq!(trigger_tag(&CheckpointTrigger::OnPause), "trigger:ON_PAUSE");
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
        assert_eq!(custom.get(FORMAT_VERSION_FIELD), Some(&serde_json::json!("1.1.0")));
        assert!(custom.get(CREATED_AT_FIELD).is_some());
        assert_eq!(custom.get("node_id"), Some(&serde_json::json!("node-1")));
    }

    #[test]
    fn build_checkpoint_metadata_none_without_content() {
        assert!(build_checkpoint_metadata(None, vec![], HashMap::new(), "1.1.0").is_none());
    }
}

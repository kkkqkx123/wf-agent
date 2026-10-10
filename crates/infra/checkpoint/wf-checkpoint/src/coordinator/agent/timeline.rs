use crate::coordinator::agent::AgentCheckpointCoordinator;
use checkpoint_base::error::CheckpointError;
use checkpoint_base::metadata::builder::{ITERATION_FIELD, MSG_SEQ_END_FIELD, MSG_SEQ_START_FIELD};
use checkpoint_state::state::CheckpointStateManager;
use wf_types::checkpoint::CheckpointType;

/// One timeline anchor: checkpoint id, sequence bounds, trigger label,
/// wall-clock timestamp.
pub type TimelineRow = (
    String,
    Option<u64>,
    Option<u64>,
    Option<String>,
    Option<i64>,
);

/// Timeline fields read from storage metadata without loading the blob.
/// Returns `None` for rows predating the coordinate fields so the caller
/// falls back to the snapshot body.
pub(super) fn timeline_from_metadata(
    meta: &wf_types::storage::CheckpointStorageMetadata,
) -> Option<(Option<u64>, Option<u64>, Option<String>)> {
    let fields = meta.custom_fields.as_ref()?;
    fields.get(ITERATION_FIELD)?.as_u64()?;
    let start = fields.get(MSG_SEQ_START_FIELD).and_then(|v| v.as_u64());
    let end = fields.get(MSG_SEQ_END_FIELD).and_then(|v| v.as_u64());
    Some((start, end, trigger_label_from_tags(meta.tags.as_ref())))
}

/// Trigger label resolved through the single wire table in
/// `checkpoint_base::metadata::builder`: the stored `trigger:<name>` tag
/// maps back to its trigger and renders the same description `build`
/// injects into the blob metadata. A present-but-unknown wire name renders
/// an explicit unknown marker instead of empty, so a timeline row never
/// silently loses its trigger to table drift.
fn trigger_label_from_tags(tags: Option<&Vec<String>>) -> Option<String> {
    use checkpoint_base::metadata::builder::{trigger_description, trigger_from_wire_name};
    let wire = tags?.iter().find_map(|tag| tag.strip_prefix("trigger:"))?;
    match trigger_from_wire_name(wire) {
        Some(trigger) => Some(trigger_description(&trigger)),
        None => Some(format!("Unknown trigger ({wire})")),
    }
}

/// Sequence bound read from a blob metadata map via its `customFields`
/// object. The previous timeline path read the top-level map and always
/// missed delta rows.
fn timeline_custom_u64(
    metadata: &Option<std::collections::HashMap<String, serde_json::Value>>,
    key: &str,
) -> Option<u64> {
    metadata
        .as_ref()
        .and_then(|m| m.get("customFields"))
        .and_then(|v| v.as_object())
        .and_then(|custom| custom.get(key))
        .and_then(|v| v.as_u64())
}

impl AgentCheckpointCoordinator {
    /// Timeline anchors for one execution, ordered by sequence end.
    /// Sequence bounds prefer checkpoint metadata so no blob is loaded for
    /// new rows; rows predating the coordinate fields fall back to the
    /// snapshot body. Rows still missing a bound sort last by an explicit
    /// maximum fallback: the order stays deterministic, but a trailing row
    /// signals missing coordinates rather than a late sequence.
    pub async fn timeline(&self, entity_id: &str) -> Result<Vec<TimelineRow>, CheckpointError> {
        let metas = self.state_manager.list_by_entity(entity_id).await?;
        let mut entries: Vec<TimelineRow> = Vec::new();
        for meta in metas {
            if let Some((start, end, trigger)) = timeline_from_metadata(&meta) {
                entries.push((meta.id, start, end, trigger, Some(meta.timestamp)));
                continue;
            }
            let full = self.state_manager.load(&meta.id).await?;
            let Some(cp) = full else { continue };
            let (start, end) = match cp.r#type {
                Some(CheckpointType::Full) => (
                    cp.snapshot.as_ref().and_then(|s| s.message_seq_start),
                    cp.snapshot.as_ref().and_then(|s| s.message_seq_end),
                ),
                _ => (
                    timeline_custom_u64(&cp.metadata, MSG_SEQ_START_FIELD),
                    timeline_custom_u64(&cp.metadata, MSG_SEQ_END_FIELD),
                ),
            };
            let trigger = cp
                .metadata
                .as_ref()
                .and_then(|m| m.get("description"))
                .and_then(|v| v.as_str())
                .map(String::from);
            entries.push((cp.id, start, end, trigger, cp.timestamp));
        }
        entries.sort_by_key(|e| (e.2.unwrap_or(u64::MAX), e.4.unwrap_or(i64::MAX)));
        Ok(entries)
    }
}

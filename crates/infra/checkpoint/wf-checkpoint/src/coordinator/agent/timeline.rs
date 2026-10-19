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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::base::CheckpointCoordinator;
    use checkpoint_base::metadata::builder::{
        trigger_description, ITERATION_FIELD, MSG_SEQ_END_FIELD, MSG_SEQ_START_FIELD,
    };
    use checkpoint_state::state::AgentCheckpointStateManager;
    use std::collections::HashMap;
    use std::sync::Arc;
    use wf_storage::backend::StorageBackend;
    use wf_types::checkpoint::agent::AgentStateSnapshot;
    use wf_types::checkpoint::{CheckpointStatus, CheckpointType, CheckpointTiming};
    use wf_types::storage::CheckpointStorageMetadata;

    fn meta_with(
        custom_fields: Option<HashMap<String, serde_json::Value>>,
        tags: Option<Vec<String>>,
    ) -> CheckpointStorageMetadata {
        CheckpointStorageMetadata {
            id: "row-1".to_string(),
            entity_type: "agent_loop".to_string(),
            entity_id: "loop-1".to_string(),
            parent_entity_id: None,
            checkpoint_type: CheckpointType::Full,
            timestamp: 1000,
            status: CheckpointStatus::Completed,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: Some(0),
            blob_size: None,
            tags,
            custom_fields,
        }
    }

    #[test]
    fn timeline_from_metadata_reads_coordinates_and_trigger() {
        let mut fields = HashMap::new();
        fields.insert(ITERATION_FIELD.to_string(), serde_json::json!(3));
        fields.insert(MSG_SEQ_START_FIELD.to_string(), serde_json::json!(10));
        fields.insert(MSG_SEQ_END_FIELD.to_string(), serde_json::json!(12));
        let meta = meta_with(
            Some(fields),
            Some(vec!["trigger:AFTER_EXECUTE".to_string()]),
        );
        assert_eq!(
            timeline_from_metadata(&meta),
            Some((
                Some(10),
                Some(12),
                Some(trigger_description(&CheckpointTiming::AfterExecute))
            ))
        );

        // Iteration alone still marks a coordinate row without sequence bounds.
        let fields = HashMap::from([(ITERATION_FIELD.to_string(), serde_json::json!(3))]);
        let meta = meta_with(Some(fields), None);
        assert_eq!(
            timeline_from_metadata(&meta),
            Some((None, None, None))
        );

        // Rows predating the coordinate fields fall back to the snapshot body.
        assert_eq!(timeline_from_metadata(&meta_with(None, None)), None);
        let fields = HashMap::from([(MSG_SEQ_END_FIELD.to_string(), serde_json::json!(9))]);
        assert_eq!(timeline_from_metadata(&meta_with(Some(fields), None)), None);
    }

    #[test]
    fn timeline_from_metadata_marks_unknown_trigger_wires() {
        let fields = HashMap::from([(ITERATION_FIELD.to_string(), serde_json::json!(3))]);
        let meta = meta_with(
            Some(fields),
            Some(vec!["trigger:NO_SUCH".to_string()]),
        );
        assert_eq!(
            timeline_from_metadata(&meta),
            Some((
                None,
                None,
                Some("Unknown trigger (NO_SUCH)".to_string())
            ))
        );
    }

    fn snapshot(iteration: u32, start: Option<u64>, end: Option<u64>) -> AgentStateSnapshot {
        AgentStateSnapshot {
            agent_loop_id: "loop-1".to_string(),
            status: "running".to_string(),
            current_iteration: iteration,
            tool_call_count: 0,
            conversation_snapshot: None,
            conversation_view: None,
            message_seq_start: start,
            message_seq_end: end,
            message_next_seq: None,
            conversation_ledger: None,
            conversation_tracker: None,
            tool_call_history: None,
            is_streaming: None,
            variable_snapshots: None,
            error: None,
            started_at: None,
            completed_at: None,
            error_records: None,
            retry_totals: None,
            interruption_records: None,
            event_records: None,
            iteration_history: None,
            current_iteration_record: None,
            stream_message: None,
            pending_tool_call_ids: None,
            trigger_state: None,
            hierarchy: None,
            messages: None,
            tool_discovery_state: None,
            permanently_failed_tools: None,
            loop_config: None,
        }
    }

    #[tokio::test]
    async fn timeline_orders_rows_by_sequence_end_and_labels_triggers() {
        let sm = AgentCheckpointStateManager::new(Arc::new(StorageBackend::new_memory()));
        let coord = AgentCheckpointCoordinator::new(sm);
        let coord = &coord;

        async fn persist(
            coord: &AgentCheckpointCoordinator,
            iter: u32,
            start: Option<u64>,
            end: Option<u64>,
        ) -> String {
            let ctx = coord
                .prepare("loop-1", CheckpointTiming::AfterExecute)
                .await
                .unwrap();
            let cp = coord.build(ctx, snapshot(iter, start, end)).await.unwrap();
            coord.persist(&cp, "loop-1").await.unwrap();
            cp.id
        }

        // Persist the newest row first: order must come from the sequence
        // coordinates, not from insertion order.
        let newest = persist(coord, 3, Some(20), None).await;
        let mid = persist(coord, 2, Some(10), Some(20)).await;
        let first = persist(coord, 1, Some(0), Some(10)).await;

        let rows = coord.timeline("loop-1").await.unwrap();
        let label = Some(trigger_description(&CheckpointTiming::AfterExecute));
        assert_eq!(
            rows.iter()
                .map(|r| (r.0.clone(), r.1, r.2, r.3.clone()))
                .collect::<Vec<_>>(),
            vec![
                (first, Some(0), Some(10), label.clone()),
                (mid, Some(10), Some(20), label.clone()),
                // A row without a sequence end sorts last, not by timestamp.
                (newest, Some(20), None, label),
            ]
        );
        assert!(rows.iter().all(|r| r.4.is_some()));
    }
}

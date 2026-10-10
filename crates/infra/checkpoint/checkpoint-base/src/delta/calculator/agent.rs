use crate::delta::DiffCalculator;
use crate::error::CheckpointError;

pub struct AgentDiffCalculator;

impl AgentDiffCalculator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentDiffCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl
    DiffCalculator<
        wf_types::checkpoint::agent::AgentStateSnapshot,
        wf_types::checkpoint::agent::AgentCheckpointDelta,
    > for AgentDiffCalculator
{
    async fn calculate_diff(
        &self,
        previous: &wf_types::checkpoint::agent::AgentStateSnapshot,
        current: &wf_types::checkpoint::agent::AgentStateSnapshot,
    ) -> Result<wf_types::checkpoint::agent::AgentCheckpointDelta, CheckpointError> {
        use wf_types::checkpoint::agent::AgentCheckpointDelta;

        let (added_messages, added_message_base_seq) =
            Self::diff_messages_append_only(previous, current);

        let added_iterations = if current.current_iteration != previous.current_iteration {
            Some(vec![current.current_iteration])
        } else {
            None
        };

        let status_change = if current.status != previous.status {
            Some(wf_types::checkpoint::FieldChange {
                from: Some(previous.status.clone()),
                to: Some(current.status.clone()),
            })
        } else {
            None
        };

        Ok(AgentCheckpointDelta {
            added_messages,
            added_message_base_seq,
            added_iterations,
            status_change,
            other_changes: Self::diff_other_changes(previous, current),
        })
    }

    async fn apply_delta(
        &self,
        base: &wf_types::checkpoint::agent::AgentStateSnapshot,
        delta: &wf_types::checkpoint::agent::AgentCheckpointDelta,
    ) -> Result<wf_types::checkpoint::agent::AgentStateSnapshot, CheckpointError> {
        let mut result = base.clone();

        if let Some(ref messages) = delta.added_messages {
            match delta.added_message_base_seq {
                Some(base_seq) => {
                    let base_start = base.message_seq_start.unwrap_or(0);
                    // Sequence rollback or reorder indicates a corrupted or
                    // mismatched base: truncating on it would splice the wrong
                    // prefix, so fail loudly instead of merging.
                    if base_seq < base_start {
                        return Err(CheckpointError::Corrupted {
                            id: String::new(),
                            reason: format!(
                                "agent delta base_seq {base_seq} precedes base start {base_start}; refusing suffix merge"
                            ),
                        });
                    }
                    let keep = base_seq.saturating_sub(base_start) as usize;
                    let mut merged = base.conversation_snapshot.clone().unwrap_or_default();
                    if keep > merged.len() {
                        return Err(CheckpointError::Corrupted {
                            id: String::new(),
                            reason: format!(
                                "agent delta base_seq {base_seq} exceeds base history length {}; refusing suffix merge",
                                merged.len()
                            ),
                        });
                    }
                    merged.truncate(keep);
                    merged.extend(messages.clone());
                    result.conversation_snapshot = Some(merged);
                }
                None => {
                    result.conversation_snapshot = Some(messages.clone());
                }
            }
        }

        if let Some(ref iters) = delta.added_iterations {
            if let Some(&last) = iters.last() {
                result.current_iteration = last;
            }
        }

        if let Some(ref status) = delta.status_change {
            if let Some(s) = status.to.as_ref() {
                result.status = s.clone();
            }
        }

        if let Some(ref other) = delta.other_changes {
            if other.contains_key("tool_call_count") {
                if let Some(v) = other.get("tool_call_count").and_then(|v| v.as_u64()) {
                    result.tool_call_count = v as u32;
                }
            }
            if other.contains_key("tool_call_history") {
                result.tool_call_history = other
                    .get("tool_call_history")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("is_streaming") {
                result.is_streaming = other.get("is_streaming").and_then(|v| v.as_bool());
            }
            if other.contains_key("variable_snapshots") {
                result.variable_snapshots = other
                    .get("variable_snapshots")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("error") {
                result.error = other
                    .get("error")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| v.as_str().map(String::from));
            }
            if other.contains_key("started_at") {
                result.started_at = other
                    .get("started_at")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| v.as_i64());
            }
            if other.contains_key("completed_at") {
                result.completed_at = other
                    .get("completed_at")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| v.as_i64());
            }
            if other.contains_key("retry_totals") {
                result.retry_totals = other
                    .get("retry_totals")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            for key in [
                "error_records",
                "interruption_records",
                "event_records",
                "iteration_history",
                "current_iteration_record",
                "trigger_state",
            ] {
                if other.contains_key(key) {
                    let parsed: Option<serde_json::Value> = match other.get(key) {
                        Some(v) if !v.is_null() => Some(v.clone()),
                        _ => None,
                    };
                    let as_vec = parsed.as_ref().and_then(|v| {
                        serde_json::from_value::<Vec<serde_json::Value>>(v.clone()).ok()
                    });
                    match key {
                        "error_records" => result.error_records = as_vec,
                        "interruption_records" => result.interruption_records = as_vec,
                        "event_records" => result.event_records = as_vec,
                        "iteration_history" => result.iteration_history = as_vec,
                        "current_iteration_record" => result.current_iteration_record = parsed,
                        "trigger_state" => result.trigger_state = parsed,
                        _ => {}
                    }
                }
            }
            if other.contains_key("stream_message") {
                result.stream_message = other
                    .get("stream_message")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| v.as_str().map(String::from));
            }
            if other.contains_key("pending_tool_call_ids") {
                result.pending_tool_call_ids = other
                    .get("pending_tool_call_ids")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("hierarchy") {
                result.hierarchy = other
                    .get("hierarchy")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("messages") {
                result.messages = other
                    .get("messages")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("conversation_view") {
                result.conversation_view = other
                    .get("conversation_view")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("message_seq_start") {
                result.message_seq_start = other
                    .get("message_seq_start")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("message_seq_end") {
                result.message_seq_end = other
                    .get("message_seq_end")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("message_next_seq") {
                result.message_next_seq = other
                    .get("message_next_seq")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("conversation_ledger") {
                result.conversation_ledger = other
                    .get("conversation_ledger")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("conversation_tracker") {
                result.conversation_tracker = other
                    .get("conversation_tracker")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
        }

        Ok(result)
    }
}

impl AgentDiffCalculator {
    fn diff_other_changes(
        previous: &wf_types::checkpoint::agent::AgentStateSnapshot,
        current: &wf_types::checkpoint::agent::AgentStateSnapshot,
    ) -> Option<wf_types::Metadata> {
        let mut other = wf_types::Metadata::new();

        if current.tool_call_count != previous.tool_call_count {
            other.insert(
                "tool_call_count".to_string(),
                serde_json::json!(current.tool_call_count),
            );
        }
        if current.tool_call_history != previous.tool_call_history {
            other.insert(
                "tool_call_history".to_string(),
                serde_json::to_value(&current.tool_call_history).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.is_streaming != previous.is_streaming {
            other.insert(
                "is_streaming".to_string(),
                serde_json::to_value(current.is_streaming).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.variable_snapshots != previous.variable_snapshots {
            other.insert(
                "variable_snapshots".to_string(),
                serde_json::to_value(&current.variable_snapshots)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.error != previous.error {
            other.insert(
                "error".to_string(),
                serde_json::to_value(&current.error).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.started_at != previous.started_at {
            other.insert(
                "started_at".to_string(),
                serde_json::to_value(current.started_at).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.completed_at != previous.completed_at {
            other.insert(
                "completed_at".to_string(),
                serde_json::to_value(current.completed_at).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.error_records != previous.error_records {
            other.insert(
                "error_records".to_string(),
                serde_json::to_value(&current.error_records).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.retry_totals != previous.retry_totals {
            other.insert(
                "retry_totals".to_string(),
                serde_json::to_value(&current.retry_totals).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.interruption_records != previous.interruption_records {
            other.insert(
                "interruption_records".to_string(),
                serde_json::to_value(&current.interruption_records)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.event_records != previous.event_records {
            other.insert(
                "event_records".to_string(),
                serde_json::to_value(&current.event_records).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.iteration_history != previous.iteration_history {
            other.insert(
                "iteration_history".to_string(),
                serde_json::to_value(&current.iteration_history).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.current_iteration_record != previous.current_iteration_record {
            other.insert(
                "current_iteration_record".to_string(),
                serde_json::to_value(&current.current_iteration_record)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.stream_message != previous.stream_message {
            other.insert(
                "stream_message".to_string(),
                serde_json::to_value(&current.stream_message).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.pending_tool_call_ids != previous.pending_tool_call_ids {
            other.insert(
                "pending_tool_call_ids".to_string(),
                serde_json::to_value(&current.pending_tool_call_ids)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.trigger_state != previous.trigger_state {
            other.insert(
                "trigger_state".to_string(),
                serde_json::to_value(&current.trigger_state).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.hierarchy != previous.hierarchy {
            other.insert(
                "hierarchy".to_string(),
                serde_json::to_value(&current.hierarchy).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.messages != previous.messages {
            other.insert(
                "messages".to_string(),
                serde_json::to_value(&current.messages).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.conversation_view != previous.conversation_view {
            other.insert(
                "conversation_view".to_string(),
                serde_json::to_value(&current.conversation_view).unwrap_or(serde_json::Value::Null),
            );
        }
        for key in [
            "message_seq_start",
            "message_seq_end",
            "message_next_seq",
            "conversation_ledger",
            "conversation_tracker",
        ] {
            let prev_val = match key {
                "message_seq_start" => serde_json::to_value(previous.message_seq_start),
                "message_seq_end" => serde_json::to_value(previous.message_seq_end),
                "message_next_seq" => serde_json::to_value(previous.message_next_seq),
                "conversation_ledger" => serde_json::to_value(&previous.conversation_ledger),
                "conversation_tracker" => serde_json::to_value(&previous.conversation_tracker),
                _ => Ok(serde_json::Value::Null),
            }
            .unwrap_or(serde_json::Value::Null);
            let curr_val = match key {
                "message_seq_start" => serde_json::to_value(current.message_seq_start),
                "message_seq_end" => serde_json::to_value(current.message_seq_end),
                "message_next_seq" => serde_json::to_value(current.message_next_seq),
                "conversation_ledger" => serde_json::to_value(&current.conversation_ledger),
                "conversation_tracker" => serde_json::to_value(&current.conversation_tracker),
                _ => Ok(serde_json::Value::Null),
            }
            .unwrap_or(serde_json::Value::Null);
            if prev_val != curr_val {
                other.insert(key.to_string(), curr_val);
            }
        }

        (!other.is_empty()).then_some(other)
    }

    /// Append-only message diff: when the current history extends the previous
    /// one (common prefix by id), only the suffix is stored with its base
    /// sequence. Otherwise a legacy full replacement is emitted.
    ///
    /// The comparison borrows both sides instead of cloning whole message
    /// histories: these snapshots carry the entire conversation, and cloning
    /// them here doubles the peak memory held across the surrounding async
    /// checkpoint stack. Only the emitted suffix is materialized.
    fn diff_messages_append_only(
        previous: &wf_types::checkpoint::agent::AgentStateSnapshot,
        current: &wf_types::checkpoint::agent::AgentStateSnapshot,
    ) -> (Option<Vec<wf_types::message::Message>>, Option<u64>) {
        let prev: &[wf_types::message::Message] =
            previous.conversation_snapshot.as_deref().unwrap_or(&[]);
        let curr: &[wf_types::message::Message] =
            current.conversation_snapshot.as_deref().unwrap_or(&[]);
        if prev == curr {
            return (None, None);
        }
        let mut common = 0usize;
        while common < prev.len() && common < curr.len() && prev[common].id == curr[common].id {
            common += 1;
        }
        if common > 0 || prev.is_empty() {
            let base_seq = previous
                .message_seq_start
                .unwrap_or(0)
                .saturating_add(common as u64);
            let suffix = curr[common..].to_vec();
            if suffix.is_empty() {
                return (None, None);
            }
            return (Some(suffix), Some(base_seq));
        }
        (Some(curr.to_vec()), None)
    }
}

use crate::delta::DiffCalculator;
use crate::error::CheckpointError;

/// Result of a message diff: (added, modified, deleted indices).
type MessageDiff = (
    Option<Vec<wf_types::message::Message>>,
    Option<Vec<wf_types::message::Message>>,
    Option<Vec<u32>>,
);

/// Marker for a deleted variable in a workflow delta. Deletions must not
/// reuse JSON null: null is a legitimate variable value, so a dedicated
/// object marker keeps explicit nulls round-tripping while deletions remove
/// the key on apply.
pub const VARIABLE_DELETED_MARKER_KEY: &str = "__wf_variable_deleted__";

fn is_variable_deleted_marker(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|obj| {
        obj.get(VARIABLE_DELETED_MARKER_KEY)
            .is_some_and(|v| v == &serde_json::Value::Bool(true))
    })
}

fn variable_deleted_marker() -> serde_json::Value {
    serde_json::json!({ VARIABLE_DELETED_MARKER_KEY: true })
}

pub struct WorkflowDiffCalculator;

impl WorkflowDiffCalculator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WorkflowDiffCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl
    DiffCalculator<
        wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
        wf_types::checkpoint::workflow::WorkflowCheckpointDelta,
    > for WorkflowDiffCalculator
{
    async fn calculate_diff(
        &self,
        previous: &wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
        current: &wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
    ) -> Result<wf_types::checkpoint::workflow::WorkflowCheckpointDelta, CheckpointError> {
        use wf_types::checkpoint::workflow::WorkflowCheckpointDelta;

        let (added_messages, modified_messages, deleted_message_indices) =
            Self::diff_messages(&previous.messages, &current.messages);

        let (added_variables, modified_variables) =
            Self::diff_variables(&previous.variable_state, &current.variable_state);

        let message_contexts = Self::diff_message_contexts(
            current.message_contexts.as_ref(),
            previous.message_contexts.as_ref(),
        );

        let (added_node_results, modified_node_results) = Self::diff_node_results(
            previous.node_results.as_ref(),
            current.node_results.as_ref(),
        );

        let status_change = if current.status != previous.status {
            Some(wf_types::checkpoint::FieldChange {
                from: Some(previous.status.clone()),
                to: Some(current.status.clone()),
            })
        } else {
            None
        };

        let current_node_change = if current.current_node_id != previous.current_node_id {
            Some(wf_types::checkpoint::FieldChange {
                from: previous.current_node_id.clone(),
                to: current.current_node_id.clone(),
            })
        } else {
            None
        };

        let other_changes = Self::diff_other_changes(previous, current);

        Ok(WorkflowCheckpointDelta {
            added_messages,
            modified_messages,
            deleted_message_indices,
            added_variables,
            modified_variables,
            message_contexts,
            added_node_results,
            modified_node_results,
            status_change,
            current_node_change,
            other_changes,
        })
    }

    async fn apply_delta(
        &self,
        base: &wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
        delta: &wf_types::checkpoint::workflow::WorkflowCheckpointDelta,
    ) -> Result<wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot, CheckpointError>
    {
        let mut result = base.clone();

        if delta.added_messages.is_some()
            || delta.modified_messages.is_some()
            || delta.deleted_message_indices.is_some()
        {
            let mut messages = result.messages.take().unwrap_or_default();

            if let Some(ref added) = delta.added_messages {
                messages.extend(added.iter().cloned());
            }

            if let Some(ref modified) = delta.modified_messages {
                for message in modified {
                    if let Some(idx) = messages.iter().position(|m| m.id == message.id) {
                        messages[idx] = message.clone();
                    }
                }
            }

            if let Some(ref deleted) = delta.deleted_message_indices {
                let mut indices: Vec<usize> = deleted.iter().map(|idx| *idx as usize).collect();
                indices.sort_unstable_by(|a, b| b.cmp(a));
                for idx in indices {
                    if idx < messages.len() {
                        messages.remove(idx);
                    }
                }
            }

            result.messages = Some(messages);
        }

        if let Some(ref status) = delta.status_change {
            if let Some(s) = status.to.as_ref() {
                result.status = s.clone();
            }
        }

        if let Some(ref node_change) = delta.current_node_change {
            if let Some(node_id) = node_change.to.as_ref() {
                result.current_node_id = Some(node_id.clone());
            }
        }

        if let Some(ref node_results) = delta.added_node_results {
            match serde_json::from_value(node_results.clone()) {
                Ok(parsed) => result.node_results = Some(parsed),
                Err(err) => {
                    return Err(CheckpointError::Corrupted {
                        id: String::new(),
                        reason: format!(
                            "node results deserialization failed while applying delta: {err}"
                        ),
                    });
                }
            }
        }

        if let Some(ref modified) = delta.modified_node_results {
            let mut map = result.node_results.take().unwrap_or_default();
            for (node_id, value) in modified {
                map.insert(node_id.clone(), value.clone());
            }
            result.node_results = Some(map);
        }

        if let Some(ref contexts) = delta.message_contexts {
            let mut map = result.message_contexts.take().unwrap_or_default();
            for (context_id, context_delta) in contexts {
                let entry = map.entry(context_id.clone()).or_insert_with(|| {
                    wf_types::checkpoint::workflow::MessageContextSnapshot {
                        messages: Vec::new(),
                        version: 0,
                    }
                });
                for message in &context_delta.added_messages {
                    if !entry.messages.iter().any(|m| m.id == message.id) {
                        entry.messages.push(message.clone());
                    }
                }
            }
            result.message_contexts = Some(map);
        }

        if let Some(ref vars) = delta.added_variables {
            for (name, value) in vars {
                result
                    .variable_state
                    .variables
                    .insert(name.clone(), value.clone());
            }
        }

        if let Some(ref vars) = delta.modified_variables {
            for (name, value) in vars {
                if is_variable_deleted_marker(value) {
                    result.variable_state.variables.remove(name);
                } else {
                    result
                        .variable_state
                        .variables
                        .insert(name.clone(), value.clone());
                }
            }
        }

        if let Some(ref other) = delta.other_changes {
            if other.contains_key("input") {
                result.input = other
                    .get("input")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()));
            }
            if other.contains_key("output") {
                result.output = other
                    .get("output")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()));
            }
            if other.contains_key("fork_join_context") {
                result.fork_join_context = other
                    .get("fork_join_context")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()));
            }
            if other.contains_key("active_operations") {
                if let Some(v) = other.get("active_operations") {
                    result.active_operations = if v.is_null() {
                        None
                    } else {
                        serde_json::from_value(v.clone()).ok()
                    };
                }
            }
            if other.contains_key("conversation_state") {
                result.conversation_state = other
                    .get("conversation_state")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()));
            }
            if other.contains_key("trigger_states") {
                result.trigger_states = other
                    .get("trigger_states")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()));
            }
            for key in [
                "error_records",
                "interruption_records",
                "event_records",
                "execution_config",
                "fork_join_aggregation_state",
                "hook_execution_context",
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
                        "execution_config" => result.execution_config = parsed,
                        "fork_join_aggregation_state" => {
                            result.fork_join_aggregation_state = parsed
                        }
                        "hook_execution_context" => result.hook_execution_context = parsed,
                        _ => {}
                    }
                }
            }
            if other.contains_key("hierarchy") {
                result.hierarchy = other
                    .get("hierarchy")
                    .and_then(|v| (!v.is_null()).then(|| v.clone()))
                    .and_then(|v| serde_json::from_value(v).ok());
            }
            if other.contains_key("error_suspend") {
                result.error_suspend = other
                    .get("error_suspend")
                    .filter(|v| !v.is_null())
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
            }
        }

        Ok(result)
    }
}

impl WorkflowDiffCalculator {
    fn diff_messages(
        previous: &Option<Vec<wf_types::message::Message>>,
        current: &Option<Vec<wf_types::message::Message>>,
    ) -> MessageDiff {
        use wf_types::message::Message;

        match (previous, current) {
            (None, None) => (None, None, None),
            (None, Some(curr)) => (Some(curr.clone()), None, None),
            (Some(_), None) => {
                let indices: Vec<u32> = previous
                    .as_ref()
                    .map(|prev| (0..prev.len() as u32).collect())
                    .unwrap_or_default();
                (None, None, (!indices.is_empty()).then_some(indices))
            }
            (Some(prev), Some(curr)) => {
                let mut added: Vec<Message> = Vec::new();
                let mut modified: Vec<Message> = Vec::new();
                let mut deleted: Vec<u32> = Vec::new();

                for (idx, message) in prev.iter().enumerate() {
                    match curr.iter().find(|c| c.id == message.id) {
                        Some(current_message) => {
                            if current_message != message {
                                modified.push(current_message.clone());
                            }
                        }
                        None => deleted.push(idx as u32),
                    }
                }

                for message in curr.iter() {
                    if !prev.iter().any(|p| p.id == message.id) {
                        added.push(message.clone());
                    }
                }

                (
                    (!added.is_empty()).then_some(added),
                    (!modified.is_empty()).then_some(modified),
                    (!deleted.is_empty()).then_some(deleted),
                )
            }
        }
    }

    fn diff_variables(
        previous: &wf_types::checkpoint::CheckpointVariableState,
        current: &wf_types::checkpoint::CheckpointVariableState,
    ) -> (Option<wf_types::Metadata>, Option<wf_types::Metadata>) {
        let mut added = wf_types::Metadata::new();
        let mut modified = wf_types::Metadata::new();

        for (name, value) in current.variables.iter() {
            match previous.variables.get(name) {
                None => {
                    added.insert(name.clone(), value.clone());
                }
                Some(prev_value) => {
                    if prev_value != value {
                        modified.insert(name.clone(), value.clone());
                    }
                }
            }
        }

        for name in previous.variables.keys() {
            if !current.variables.contains_key(name) {
                modified.insert(name.clone(), variable_deleted_marker());
            }
        }

        (
            (!added.is_empty()).then_some(added),
            (!modified.is_empty()).then_some(modified),
        )
    }

    /// Diff named message contexts as an append-only, message-id-deduplicated
    /// set: for each context present in `current`, emit only the messages
    /// whose ids are not already in the `previous` context. Contexts are never
    /// dropped or replaced, so applying the delta extends base history.
    fn diff_message_contexts(
        current: Option<
            &std::collections::HashMap<
                String,
                wf_types::checkpoint::workflow::MessageContextSnapshot,
            >,
        >,
        previous: Option<
            &std::collections::HashMap<
                String,
                wf_types::checkpoint::workflow::MessageContextSnapshot,
            >,
        >,
    ) -> Option<
        std::collections::HashMap<String, wf_types::checkpoint::workflow::MessageContextDelta>,
    > {
        use std::collections::{HashMap, HashSet};
        use wf_types::checkpoint::workflow::MessageContextDelta;

        let current = current?;
        let empty = HashMap::new();
        let previous = previous.unwrap_or(&empty);
        let mut out: HashMap<String, MessageContextDelta> = HashMap::new();

        for (context_id, curr_ctx) in current {
            let prev_ids: HashSet<&str> = previous
                .get(context_id)
                .map(|ctx| ctx.messages.iter().map(|m| m.id.as_str()).collect())
                .unwrap_or_default();
            let added: Vec<wf_types::message::Message> = curr_ctx
                .messages
                .iter()
                .filter(|m| !prev_ids.contains(m.id.as_str()))
                .cloned()
                .collect();
            if !added.is_empty() {
                out.insert(
                    context_id.clone(),
                    MessageContextDelta {
                        added_messages: added,
                    },
                );
            }
        }

        (!out.is_empty()).then_some(out)
    }

    /// Diff node results at the key level: when both sides carry a map, emit
    /// only changed/added keys as `modified` (unchanged nodes are kept by the
    /// base). A wholesale swap (or first appearance) is emitted as `added`.
    fn diff_node_results(
        previous: Option<&std::collections::HashMap<String, serde_json::Value>>,
        current: Option<&std::collections::HashMap<String, serde_json::Value>>,
    ) -> (
        Option<serde_json::Value>,
        Option<std::collections::HashMap<String, serde_json::Value>>,
    ) {
        match (previous, current) {
            (Some(prev), Some(curr)) => {
                let mut modified = std::collections::HashMap::new();
                for (node_id, value) in curr {
                    if prev.get(node_id) != Some(value) {
                        modified.insert(node_id.clone(), value.clone());
                    }
                }
                (None, (!modified.is_empty()).then_some(modified))
            }
            (None, Some(curr)) => (
                Some(serde_json::Value::Object(
                    curr.clone().into_iter().collect(),
                )),
                None,
            ),
            _ => (None, None),
        }
    }

    fn diff_other_changes(
        previous: &wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
        current: &wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
    ) -> Option<wf_types::Metadata> {
        let mut other = wf_types::Metadata::new();

        if current.input != previous.input {
            other.insert(
                "input".to_string(),
                current.input.clone().unwrap_or(serde_json::Value::Null),
            );
        }
        if current.output != previous.output {
            other.insert(
                "output".to_string(),
                current.output.clone().unwrap_or(serde_json::Value::Null),
            );
        }
        if current.fork_join_context != previous.fork_join_context {
            other.insert(
                "fork_join_context".to_string(),
                current
                    .fork_join_context
                    .clone()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.active_operations != previous.active_operations {
            other.insert(
                "active_operations".to_string(),
                serde_json::to_value(&current.active_operations).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.node_execution_records != previous.node_execution_records {
            other.insert(
                "node_execution_records".to_string(),
                serde_json::to_value(&current.node_execution_records)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.conversation_state != previous.conversation_state {
            other.insert(
                "conversation_state".to_string(),
                current
                    .conversation_state
                    .clone()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.trigger_states != previous.trigger_states {
            other.insert(
                "trigger_states".to_string(),
                current
                    .trigger_states
                    .clone()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.error_records != previous.error_records {
            other.insert(
                "error_records".to_string(),
                serde_json::to_value(&current.error_records).unwrap_or(serde_json::Value::Null),
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
        if current.hierarchy != previous.hierarchy {
            other.insert(
                "hierarchy".to_string(),
                serde_json::to_value(&current.hierarchy).unwrap_or(serde_json::Value::Null),
            );
        }
        if current.execution_config != previous.execution_config {
            other.insert(
                "execution_config".to_string(),
                current
                    .execution_config
                    .clone()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.fork_join_aggregation_state != previous.fork_join_aggregation_state {
            other.insert(
                "fork_join_aggregation_state".to_string(),
                current
                    .fork_join_aggregation_state
                    .clone()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.hook_execution_context != previous.hook_execution_context {
            other.insert(
                "hook_execution_context".to_string(),
                current
                    .hook_execution_context
                    .clone()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if current.error_suspend != previous.error_suspend {
            other.insert(
                "error_suspend".to_string(),
                serde_json::to_value(&current.error_suspend).unwrap_or(serde_json::Value::Null),
            );
        }

        (!other.is_empty()).then_some(other)
    }
}

use crate::delta::DiffCalculator;
use crate::error::CheckpointError;

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

        let (added_variables, modified_variables) = Self::diff_variables(
            &previous.variable_state,
            &current.variable_state,
        );

        let added_node_results = if current.node_results != previous.node_results {
            Some(serde_json::json!({
                "node_results": current.node_results,
            }))
        } else {
            None
        };

        let status_change = if current.status != previous.status {
            Some(serde_json::json!({
                "status": current.status,
            }))
        } else {
            None
        };

        let current_node_change = if current.current_node_id != previous.current_node_id {
            current.current_node_id.clone()
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
            added_node_results,
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
                let mut indices: Vec<usize> =
                    deleted.iter().map(|idx| *idx as usize).collect();
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
            if let Some(s) = status.get("status").and_then(|v| v.as_str()) {
                result.status = s.to_string();
            }
        }

        if let Some(ref node_id) = delta.current_node_change {
            result.current_node_id = Some(node_id.clone());
        }

        if let Some(ref node_results) = delta.added_node_results {
            if let Some(map) = node_results.get("node_results") {
                result.node_results = serde_json::from_value(map.clone()).ok();
            }
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
                if value.is_null() {
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
        }

        Ok(result)
    }
}

impl WorkflowDiffCalculator {
    fn diff_messages(
        previous: &Option<Vec<wf_types::message::Message>>,
        current: &Option<Vec<wf_types::message::Message>>,
    ) -> (
        Option<Vec<wf_types::message::Message>>,
        Option<Vec<wf_types::message::Message>>,
        Option<Vec<u32>>,
    ) {
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
                modified.insert(name.clone(), serde_json::Value::Null);
            }
        }

        (
            (!added.is_empty()).then_some(added),
            (!modified.is_empty()).then_some(modified),
        )
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
                serde_json::to_value(&current.active_operations)
                    .unwrap_or(serde_json::Value::Null),
            );
        }

        (!other.is_empty()).then_some(other)
    }
}

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

        let added_messages = if current.conversation_snapshot != previous.conversation_snapshot {
            current.conversation_snapshot.clone()
        } else {
            None
        };

        let added_iterations = if current.current_iteration != previous.current_iteration {
            Some(vec![current.current_iteration])
        } else {
            None
        };

        let status_change = if current.status != previous.status {
            Some(current.status.clone())
        } else {
            None
        };

        Ok(AgentCheckpointDelta {
            added_messages,
            added_iterations,
            status_change,
            other_changes: None,
        })
    }

    async fn apply_delta(
        &self,
        base: &wf_types::checkpoint::agent::AgentStateSnapshot,
        delta: &wf_types::checkpoint::agent::AgentCheckpointDelta,
    ) -> Result<wf_types::checkpoint::agent::AgentStateSnapshot, CheckpointError> {
        let mut result = base.clone();

        if let Some(ref messages) = delta.added_messages {
            result.conversation_snapshot = Some(messages.clone());
        }

        if let Some(ref iters) = delta.added_iterations {
            if let Some(&last) = iters.last() {
                result.current_iteration = last;
            }
        }

        if let Some(ref status) = delta.status_change {
            result.status = status.clone();
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::checkpoint::workflow::{
        OperationState, WorkflowExecutionStateSnapshot,
    };
    use wf_types::message::{Message, MessageContentValue, MessageRole};

    fn make_message(id: &str, text: &str) -> Message {
        Message {
            id: id.to_string(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn make_snapshot() -> WorkflowExecutionStateSnapshot {
        WorkflowExecutionStateSnapshot {
            execution_id: "e1".to_string(),
            status: "running".to_string(),
            current_node_id: None,
            node_results: None,
            variable_state: wf_types::checkpoint::CheckpointVariableState {
                variables: std::collections::HashMap::new(),
            },
            input: None,
            output: None,
            messages: None,
            fork_join_context: None,
            active_operations: None,
        }
    }

    async fn round_trip(
        prev: &WorkflowExecutionStateSnapshot,
        curr: &WorkflowExecutionStateSnapshot,
    ) -> WorkflowExecutionStateSnapshot {
        let calc = WorkflowDiffCalculator::new();
        let delta = calc.calculate_diff(prev, curr).await.unwrap();
        calc.apply_delta(prev, &delta).await.unwrap()
    }

    #[tokio::test]
    async fn workflow_diff_detects_status_change() {
        let calc = WorkflowDiffCalculator::new();
        let prev = make_snapshot();
        let curr = wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot {
            status: "completed".to_string(),
            ..prev.clone()
        };

        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert!(delta.status_change.is_some());
        assert!(delta.current_node_change.is_none());
    }

    #[tokio::test]
    async fn workflow_apply_delta_updates_status() {
        let calc = WorkflowDiffCalculator::new();
        let base = make_snapshot();
        let delta = wf_types::checkpoint::workflow::WorkflowCheckpointDelta {
            added_messages: None,
            modified_messages: None,
            deleted_message_indices: None,
            added_variables: None,
            modified_variables: None,
            added_node_results: None,
            status_change: Some(serde_json::json!({"status": "completed"})),
            current_node_change: Some("node-5".to_string()),
            other_changes: None,
        };

        let result = calc.apply_delta(&base, &delta).await.unwrap();
        assert_eq!(result.status, "completed");
        assert_eq!(result.current_node_id, Some("node-5".to_string()));
    }

    #[tokio::test]
    async fn workflow_message_add_modify_delete_round_trip() {
        let m1 = make_message("m1", "hello");
        let m2 = make_message("m2", "world");
        let m3 = make_message("m3", "third");

        let prev = WorkflowExecutionStateSnapshot {
            messages: Some(vec![m1.clone(), m2.clone()]),
            ..make_snapshot()
        };

        let m2_modified = make_message("m2", "world-modified");
        let curr = WorkflowExecutionStateSnapshot {
            messages: Some(vec![m1.clone(), m2_modified.clone(), m3.clone()]),
            ..prev.clone()
        };

        let calc = WorkflowDiffCalculator::new();
        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();

        assert_eq!(
            delta.added_messages,
            Some(vec![m3.clone()])
        );
        assert_eq!(delta.modified_messages, Some(vec![m2_modified.clone()]));
        assert!(delta.deleted_message_indices.is_none());

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.messages, curr.messages);
    }

    #[tokio::test]
    async fn workflow_message_delete_round_trip() {
        let m1 = make_message("m1", "hello");
        let m2 = make_message("m2", "world");

        let prev = WorkflowExecutionStateSnapshot {
            messages: Some(vec![m1.clone(), m2.clone()]),
            ..make_snapshot()
        };
        let curr = WorkflowExecutionStateSnapshot {
            messages: Some(vec![m1.clone()]),
            ..prev.clone()
        };

        let calc = WorkflowDiffCalculator::new();
        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert_eq!(delta.deleted_message_indices, Some(vec![1]));

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.messages, curr.messages);
    }

    #[tokio::test]
    async fn workflow_variables_add_modify_delete_round_trip() {
        use std::collections::HashMap;

        let mut prev_vars = HashMap::new();
        prev_vars.insert("a".to_string(), serde_json::json!(1));
        prev_vars.insert("b".to_string(), serde_json::json!("keep"));
        let prev = WorkflowExecutionStateSnapshot {
            variable_state: wf_types::checkpoint::CheckpointVariableState {
                variables: prev_vars,
            },
            ..make_snapshot()
        };

        let mut curr_vars = HashMap::new();
        curr_vars.insert("a".to_string(), serde_json::json!(2));
        curr_vars.insert("b".to_string(), serde_json::json!("keep"));
        curr_vars.insert("c".to_string(), serde_json::json!("new"));
        let curr = WorkflowExecutionStateSnapshot {
            variable_state: wf_types::checkpoint::CheckpointVariableState {
                variables: curr_vars,
            },
            ..prev.clone()
        };

        let restored = round_trip(&prev, &curr).await;
        assert_eq!(restored.variable_state, curr.variable_state);
    }

    #[tokio::test]
    async fn workflow_other_changes_round_trip() {
        let prev = make_snapshot();
        let curr = WorkflowExecutionStateSnapshot {
            input: Some(serde_json::json!({"prompt": "hello"})),
            output: Some(serde_json::json!({"result": 42})),
            fork_join_context: Some(serde_json::json!({"forkId": "f1"})),
            active_operations: Some(vec![OperationState {
                r#type: "execute".to_string(),
                operation_id: "op-1".to_string(),
                node_id: Some("node-1".to_string()),
                started_at: 123,
                progress: None,
                partial_result: None,
            }]),
            ..prev.clone()
        };

        let calc = WorkflowDiffCalculator::new();
        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        let other = delta.other_changes.as_ref().unwrap();
        assert!(other.contains_key("input"));
        assert!(other.contains_key("output"));
        assert!(other.contains_key("fork_join_context"));
        assert!(other.contains_key("active_operations"));

        let restored = calc.apply_delta(&prev, &delta).await.unwrap();
        assert_eq!(restored.input, curr.input);
        assert_eq!(restored.output, curr.output);
        assert_eq!(restored.fork_join_context, curr.fork_join_context);
        assert_eq!(restored.active_operations, curr.active_operations);
    }

    #[tokio::test]
    async fn workflow_field_removal_round_trip() {
        let prev = WorkflowExecutionStateSnapshot {
            input: Some(serde_json::json!({"prompt": "hello"})),
            messages: Some(vec![make_message("m1", "hi")]),
            ..make_snapshot()
        };
        let curr = WorkflowExecutionStateSnapshot {
            input: None,
            ..prev.clone()
        };

        let restored = round_trip(&prev, &curr).await;
        assert_eq!(restored.input, None);
        assert_eq!(restored.messages, Some(vec![make_message("m1", "hi")]));
    }

    #[tokio::test]
    async fn merge_deltas_equals_sequential_apply() {
        let calc = WorkflowDiffCalculator::new();
        let base = make_snapshot();
        let mid = WorkflowExecutionStateSnapshot {
            status: "running".to_string(),
            messages: Some(vec![make_message("m1", "hello")]),
            ..base.clone()
        };
        let curr = WorkflowExecutionStateSnapshot {
            status: "completed".to_string(),
            messages: Some(vec![make_message("m1", "hello"), make_message("m2", "world")]),
            ..base.clone()
        };

        let first = calc.calculate_diff(&base, &mid).await.unwrap();
        let second = calc.calculate_diff(&mid, &curr).await.unwrap();
        let merged = calc.merge_deltas(&base, &first, &second).await.unwrap();

        let direct = calc.calculate_diff(&base, &curr).await.unwrap();
        let restored_via_merge = calc.apply_delta(&base, &merged).await.unwrap();
        let restored_direct = calc.apply_delta(&base, &direct).await.unwrap();
        assert_eq!(restored_via_merge, restored_direct);
        assert_eq!(restored_via_merge.status, "completed");
        assert_eq!(
            restored_via_merge.messages,
            Some(vec![make_message("m1", "hello"), make_message("m2", "world")])
        );
    }

    #[tokio::test]
    async fn agent_diff_detects_iteration_change() {
        let calc = AgentDiffCalculator::new();
        let prev = wf_types::checkpoint::agent::AgentStateSnapshot {
            agent_loop_id: "a1".to_string(),
            status: "running".to_string(),
            current_iteration: 1,
            tool_call_count: 0,
            conversation_snapshot: None,
            tool_call_history: None,
            is_streaming: None,
            variable_snapshots: None,
            error: None,
            started_at: None,
            completed_at: None,
        };
        let curr = wf_types::checkpoint::agent::AgentStateSnapshot {
            current_iteration: 2,
            ..prev.clone()
        };

        let delta = calc.calculate_diff(&prev, &curr).await.unwrap();
        assert_eq!(delta.added_iterations, Some(vec![2]));
    }

    #[tokio::test]
    async fn agent_apply_delta_updates_iteration() {
        let calc = AgentDiffCalculator::new();
        let base = wf_types::checkpoint::agent::AgentStateSnapshot {
            agent_loop_id: "a1".to_string(),
            status: "running".to_string(),
            current_iteration: 1,
            tool_call_count: 0,
            conversation_snapshot: None,
            tool_call_history: None,
            is_streaming: None,
            variable_snapshots: None,
            error: None,
            started_at: None,
            completed_at: None,
        };
        let delta = wf_types::checkpoint::agent::AgentCheckpointDelta {
            added_messages: None,
            added_iterations: Some(vec![2, 3]),
            status_change: Some("completed".to_string()),
            other_changes: None,
        };

        let result = calc.apply_delta(&base, &delta).await.unwrap();
        assert_eq!(result.current_iteration, 3);
        assert_eq!(result.status, "completed");
    }
}

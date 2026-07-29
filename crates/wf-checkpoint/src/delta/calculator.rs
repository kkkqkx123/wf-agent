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

        let added_messages = if current.messages != previous.messages {
            current.messages.clone()
        } else {
            None
        };

        let added_variables = if current.variable_state != previous.variable_state {
            let mut map = wf_types::Metadata::new();
            map.insert(
                "variables".to_string(),
                serde_json::to_value(&current.variable_state.variables).unwrap_or_default(),
            );
            Some(map)
        } else {
            None
        };

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

        Ok(WorkflowCheckpointDelta {
            added_messages,
            modified_messages: None,
            deleted_message_indices: None,
            added_variables,
            modified_variables: None,
            added_node_results,
            status_change,
            current_node_change,
            other_changes: None,
        })
    }

    async fn apply_delta(
        &self,
        base: &wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
        delta: &wf_types::checkpoint::workflow::WorkflowCheckpointDelta,
    ) -> Result<wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot, CheckpointError>
    {
        let mut result = base.clone();

        if let Some(ref messages) = delta.added_messages {
            result.messages = Some(messages.clone());
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
            if let Ok(state) = serde_json::from_value::<wf_types::checkpoint::CheckpointVariableState>(
                serde_json::to_value(vars).unwrap_or_default(),
            ) {
                result.variable_state = state;
            }
        }

        Ok(result)
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

    #[tokio::test]
    async fn workflow_diff_detects_status_change() {
        let calc = WorkflowDiffCalculator::new();
        let prev = wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot {
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
        };
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
        let base = wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot {
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
        };
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

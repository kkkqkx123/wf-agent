use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::message::{Message, MessageContentValue, MessageRole};

pub(crate) fn make_message(id: &str, text: &str) -> Message {
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

pub(crate) fn make_workflow_snapshot() -> WorkflowExecutionStateSnapshot {
    WorkflowExecutionStateSnapshot {
        execution_id: "e1".to_string(),
        status: "running".to_string(),
        current_node_id: None,
        node_results: None,
        variable_state: wf_types::checkpoint::CheckpointVariableState {
            variables: std::collections::HashMap::new(),
        },
        message_contexts: None,
        input: None,
        output: None,
        messages: None,
        fork_join_context: None,
        active_operations: None,

        node_execution_records: None,
        conversation_state: None,
        trigger_states: None,
        error_records: None,
        interruption_records: None,
        event_records: None,
        hierarchy: None,
        execution_config: None,
        fork_join_aggregation_state: None,
        hook_execution_context: None,
        error_suspend: None,
    }
}

pub(crate) fn make_agent_snapshot(
    iteration: u32,
) -> wf_types::checkpoint::agent::AgentStateSnapshot {
    wf_types::checkpoint::agent::AgentStateSnapshot {
        agent_loop_id: "a1".to_string(),
        status: "running".to_string(),
        current_iteration: iteration,
        tool_call_count: 0,
        conversation_snapshot: None,
        conversation_view: None,
        message_seq_start: None,
        message_seq_end: None,
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

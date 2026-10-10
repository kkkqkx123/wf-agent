use super::super::*;
use checkpoint_state::state::AgentCheckpointStateManager;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::CheckpointTiming;

pub(super) fn make_snapshot() -> AgentStateSnapshot {
    AgentStateSnapshot {
        agent_loop_id: "loop-1".to_string(),
        status: "running".to_string(),
        current_iteration: 1,
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

pub(super) fn make_coordinator() -> AgentCheckpointCoordinator {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = AgentCheckpointStateManager::new(storage);
    AgentCheckpointCoordinator::new(sm)
}

pub(super) async fn build_and_persist(
    coord: &AgentCheckpointCoordinator,
    status: &str,
    iteration: u32,
) -> AgentCheckpoint {
    let mut snapshot = make_snapshot();
    snapshot.status = status.to_string();
    snapshot.current_iteration = iteration;
    let ctx = coord
        .prepare("loop-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, snapshot).await.unwrap();
    coord.persist(&cp, "loop-1").await.unwrap();
    cp
}

pub(super) fn make_policy(triggers: Vec<CheckpointTiming>) -> UnifiedCheckpointPolicy {
    UnifiedCheckpointPolicy {
        enabled: true,
        triggers,
        content: None,
        retention: None,
        error_handling: None,
    }
}

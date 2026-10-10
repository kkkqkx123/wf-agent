use super::super::*;
use checkpoint_state::state::WorkflowCheckpointStateManager;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::CheckpointTiming;

pub(super) fn make_snapshot() -> WorkflowExecutionStateSnapshot {
    WorkflowExecutionStateSnapshot {
        execution_id: "exec-1".to_string(),
        status: "running".to_string(),
        current_node_id: Some("node-1".to_string()),
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

pub(super) fn make_coordinator() -> WorkflowCheckpointCoordinator {
    let storage = Arc::new(StorageBackend::new_memory());
    let sm = WorkflowCheckpointStateManager::new(storage);
    WorkflowCheckpointCoordinator::new(sm)
}

pub(super) async fn build_and_persist(
    coord: &WorkflowCheckpointCoordinator,
    status: &str,
    node: &str,
) -> WorkflowCheckpoint {
    let mut snapshot = make_snapshot();
    snapshot.status = status.to_string();
    snapshot.current_node_id = Some(node.to_string());
    let ctx = coord
        .prepare("exec-1", CheckpointTiming::AfterExecute)
        .await
        .unwrap();
    let cp = coord.build(ctx, snapshot).await.unwrap();
    coord.persist(&cp, "exec-1").await.unwrap();
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

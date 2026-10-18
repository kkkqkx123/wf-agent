//! End-to-end checkpoint lifecycle: create a workflow checkpoint, interrupt
//! the execution (pause), restore from the paused checkpoint, and verify the
//! state plus the linked file snapshot survive the round trip.

use std::sync::Arc;

use checkpoint_file::file::{FileCheckpointManager, FileContentEntry};
use checkpoint_state::state::WorkflowCheckpointStateManager;
use wf_checkpoint::coordinator::base::CheckpointCoordinator;
use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::{CheckpointTiming, CheckpointVariableState};

fn snapshot(status: &str, node: &str) -> WorkflowExecutionStateSnapshot {
    WorkflowExecutionStateSnapshot {
        execution_id: "wf-exec-1".to_string(),
        status: status.to_string(),
        current_node_id: Some(node.to_string()),
        node_results: None,
        variable_state: CheckpointVariableState {
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

#[tokio::test]
async fn create_interrupt_restore_round_trip() {
    let storage = Arc::new(StorageBackend::new_memory());
    let state_manager = WorkflowCheckpointStateManager::new(storage);

    let file_manager = FileCheckpointManager::new_in_memory().unwrap();
    file_manager
        .create_checkpoint(
            "wf-exec-1",
            &[FileContentEntry::new(
                "src/main.rs",
                b"fn main() {}".to_vec(),
            )],
        )
        .unwrap();

    let coord = WorkflowCheckpointCoordinator::new(state_manager)
        .with_file_checkpoint_manager(file_manager.clone());

    // Run: two progress checkpoints.
    coord
        .create_checkpoint(
            CheckpointTiming::AfterExecute,
            "wf-exec-1",
            snapshot("running", "node-1"),
        )
        .await
        .unwrap();
    coord
        .create_checkpoint(
            CheckpointTiming::AfterExecute,
            "wf-exec-1",
            snapshot("running", "node-2"),
        )
        .await
        .unwrap();

    // Interrupt: a pause checkpoint at node-2.
    let paused_id = coord
        .create_checkpoint(
            CheckpointTiming::OnPause,
            "wf-exec-1",
            snapshot("paused", "node-2"),
        )
        .await
        .unwrap();

    // Restore the paused checkpoint: state and current node round-trip.
    let entity = coord.restore(&paused_id).await.unwrap();
    assert_eq!(entity.execution_id, "wf-exec-1");
    assert_eq!(entity.status, "paused");
    assert_eq!(
        entity.snapshot.current_node_id.as_deref(),
        Some("node-2"),
        "restore resumes at the interrupted node"
    );

    // The state checkpoint is linked to the exact file snapshot taken when
    // it was created, so the workspace can be brought back in step.
    let files = file_manager
        .restore_state_files("wf-exec-1", &paused_id)
        .unwrap()
        .expect("paused checkpoint carries a linked file snapshot");
    assert!(files.iter().any(|f| f.path == "src/main.rs"));
}

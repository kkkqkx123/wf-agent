//! Workflow persistence rebuild tests (`persistence::build_workflow_execution`
//! over a snapshot-restored entity): a mid-run state with completed nodes,
//! error/interruption records and timeout counts survives snapshot restore,
//! the rebuilt record reflects it, and the rebuilt entity still drives.

use std::collections::HashMap;
use std::sync::Arc;

use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_execution_shared::types::state_manager::StateManager;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::entity::WorkflowExecutionEntity;
use wf_workflow::persistence::build_workflow_execution;
use wf_workflow::state::{NodeExecutionRecord, WorkflowExecutionStateSnapshot};

fn node(id: &str, node_type: &str) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner: serde_json::json!({}),
    }
}

fn edge(source: &str, target: &str) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{source}-{target}"),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Default,
        condition: None,
        label: None,
        description: None,
        error_route: None,
    }
}

fn graph() -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes: vec![
            node("start", "START"),
            node("n1", "SCRIPT"),
            node("n2", "SCRIPT"),
            node("end", "END"),
        ],
        edges: vec![edge("start", "n1"), edge("n1", "n2"), edge("n2", "end")],
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

fn options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(false),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

fn node_record(node_id: &str, success: bool) -> NodeExecutionRecord {
    NodeExecutionRecord {
        node_id: node_id.to_string(),
        node_name: node_id.to_string(),
        node_type: "script".to_string(),
        start_time: 1_000,
        end_time: Some(1_500),
        success,
        error: if success {
            None
        } else {
            Some("node boom".to_string())
        },
        input: None,
        result: Some(serde_json::json!({"ok": success})),
        branch_id: None,
    }
}

fn error_record(message: &str) -> wf_common::error_chain::ErrorRecord {
    wf_common::error_chain::ErrorRecord::new(
        "exec-rebuild-1".to_string(),
        message.to_string(),
        None,
        None,
        None,
    )
}

/// A mid-run entity: started, first node completed, parked at the second
/// node, with one error record, interruption/event audit rows and two
/// timeout counts. Variables and node outputs live on the entity maps.
async fn mid_run_entity() -> Arc<WorkflowExecutionEntity> {
    let entity = Arc::new(WorkflowExecutionEntity::new(
        "exec-rebuild-1".to_string(),
        "wf-rebuild-1".to_string(),
    ));
    {
        let mut state = entity.state.write().await;
        state.start().expect("start");
        state.set_current_node(Some("n2".to_string()));
        state.mark_node_completed("n1".to_string());
        state.record_node_execution(node_record("n1", true));
        state.add_error_record(error_record("mid-run failure"));
        state.record_interruption(serde_json::json!({"type": "pause"}));
        state.record_event(serde_json::json!({"type": "custom"}));
        state.increment_timeout_count();
        state.increment_timeout_count();
    }
    entity.set_variable("input", serde_json::json!({"q": 1}));
    entity.set_variable("n1_out", serde_json::json!(42));
    entity.set_node_result("n1", serde_json::json!({"ok": true}));
    entity
}

fn assert_snapshots_equal(
    original: &WorkflowExecutionStateSnapshot,
    rebuilt: &WorkflowExecutionStateSnapshot,
) {
    assert_eq!(original.status, rebuilt.status);
    assert_eq!(original.current_node_id, rebuilt.current_node_id);
    assert_eq!(original.completed_nodes, rebuilt.completed_nodes);
    assert_eq!(
        original.node_execution_history.len(),
        rebuilt.node_execution_history.len()
    );
    for (left, right) in original
        .node_execution_history
        .iter()
        .zip(rebuilt.node_execution_history.iter())
    {
        assert_eq!(left.node_id, right.node_id);
        assert_eq!(left.success, right.success);
        assert_eq!(left.error, right.error);
    }
    assert_eq!(
        original
            .error_records
            .iter()
            .map(|r| r.error.clone())
            .collect::<Vec<_>>(),
        rebuilt
            .error_records
            .iter()
            .map(|r| r.error.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(original.interruption_records, rebuilt.interruption_records);
    assert_eq!(original.event_records, rebuilt.event_records);
    assert_eq!(original.timeout_count, rebuilt.timeout_count);
}

#[tokio::test]
async fn snapshot_rebuild_preserves_state_and_record() {
    let entity = mid_run_entity().await;
    let snapshot = entity
        .state
        .read()
        .await
        .create_snapshot()
        .await
        .expect("snapshot");

    // Rebuild a fresh entity purely from the snapshot plus the live maps.
    let rebuilt =
        WorkflowExecutionEntity::new("exec-rebuild-1".to_string(), "wf-rebuild-1".to_string());
    rebuilt
        .state
        .write()
        .await
        .restore_from_snapshot(snapshot)
        .await
        .expect("restore");
    rebuilt.set_variable("input", serde_json::json!({"q": 1}));
    rebuilt.set_variable("n1_out", serde_json::json!(42));
    rebuilt.set_node_result("n1", serde_json::json!({"ok": true}));

    let rebuilt_snapshot = rebuilt
        .state
        .read()
        .await
        .create_snapshot()
        .await
        .expect("rebuilt snapshot");
    let original_snapshot = entity
        .state
        .read()
        .await
        .create_snapshot()
        .await
        .expect("original snapshot");
    assert_snapshots_equal(&original_snapshot, &rebuilt_snapshot);
    assert_eq!(rebuilt_snapshot.status, ExecutionStatus::Running);
    assert_eq!(
        rebuilt_snapshot.current_node_id.as_deref(),
        Some("n2"),
        "resume pointer survives the rebuild"
    );
    assert_eq!(rebuilt_snapshot.completed_nodes, vec!["n1".to_string()]);
    assert_eq!(rebuilt_snapshot.timeout_count, 2);

    let record = build_workflow_execution(&rebuilt, &graph(), &options(), None).await;
    assert_eq!(
        record.status,
        wf_types::workflow_execution::WorkflowExecutionStatus::Running
    );
    assert_eq!(record.current_node_id.as_deref(), Some("n2"));
    let errors = record.errors.expect("error records reach the record");
    assert!(
        errors.iter().any(|e| e.contains("mid-run failure")),
        "persisted errors carry the mid-run failure: {errors:?}"
    );
    let node_ids: Vec<String> = record
        .node_results
        .expect("node outputs reach the record")
        .into_iter()
        .map(|r| r.node_id)
        .collect();
    assert!(
        node_ids.contains(&"n1".to_string()),
        "completed node outputs survive: {node_ids:?}"
    );
}

#[tokio::test]
async fn rebuilt_entity_resumes_idempotently_and_settles() {
    let entity = mid_run_entity().await;
    let snapshot = entity
        .state
        .read()
        .await
        .create_snapshot()
        .await
        .expect("snapshot");

    let rebuilt =
        WorkflowExecutionEntity::new("exec-rebuild-1".to_string(), "wf-rebuild-1".to_string());
    rebuilt
        .state
        .write()
        .await
        .restore_from_snapshot(snapshot)
        .await
        .expect("restore");

    // Idempotent re-entry: a Running restore re-drives through start.
    rebuilt
        .state
        .write()
        .await
        .start()
        .expect("resume to Running");
    assert_eq!(
        rebuilt.state.read().await.status(),
        ExecutionStatus::Running
    );

    rebuilt
        .state
        .write()
        .await
        .complete()
        .expect("settle completed");
    let record = build_workflow_execution(
        &rebuilt,
        &graph(),
        &options(),
        Some(serde_json::json!({"final": true})),
    )
    .await;
    assert_eq!(
        record.status,
        wf_types::workflow_execution::WorkflowExecutionStatus::Completed
    );
    assert!(record.completed_at.is_some(), "terminal settle lands");
    assert_eq!(record.output, Some(serde_json::json!({"final": true})));
}

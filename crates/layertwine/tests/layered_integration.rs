//! Integration tests for the layered state machine pipeline.
//!
//! These tests exercise the full five-layer pipeline end-to-end:
//!   manual_edit → agent_edit → approval → integrated → staged
//!
//! They verify correct content reconstruction, conflict detection, rollback,
//! multi-agent collaboration, and multi-feature merging.

use std::path::PathBuf;
use std::sync::Arc;

use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::{AgentInstanceId, LineDiff, SnapshotId, SourceType};
use layertwine::layered::transition::reconstruct_text;
use layertwine::layered::transition::{
    execute_forward, execute_rollback, ForwardTransition, RollbackTransition,
};
use layertwine::storage::repository::{DeltaStore, FileNodeStore, PartitionStore, SnapshotStore};
use layertwine::storage::SqliteStorage;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create an in-memory SqliteStorage with full schema (P1 + checkpoint/branch tables).
fn setup_storage() -> SqliteStorage {
    let storage = SqliteStorage::new_in_memory().expect("in-memory storage");
    storage
        .with_conn(layertwine::storage::migrations::initialize_full)
        .expect("full schema init");
    storage
}

/// Store an initial file + delta + snapshot, return the snapshot id.
fn create_initial_snapshot(storage: &SqliteStorage, content: &str) -> SnapshotId {
    let file_node = FileNode::new(PathBuf::from("test.txt"), content.as_bytes());
    storage
        .store_file_node(&file_node, content.as_bytes())
        .unwrap();
    let empty_diff = LineDiff::new(vec![]);
    let delta = Delta::new(file_node.clone(), empty_diff, SourceType::Manual);
    storage.store_delta(&delta).unwrap();
    let snapshot = Snapshot::new_initial(file_node, delta.id);
    storage.store_snapshot(&snapshot, b"").unwrap();
    snapshot.id
}

/// Reconstruct text content for a snapshot.
fn get_text(storage: &SqliteStorage, snapshot_id: &SnapshotId) -> String {
    let snap = storage.get_snapshot(snapshot_id).unwrap();
    reconstruct_text(storage, &snap).unwrap()
}

/// Ensure staged partition exists.
fn ensure_staged(storage: &SqliteStorage, initial_id: SnapshotId) {
    layertwine::layered::staged::ensure_staged_partition(storage, initial_id).unwrap();
}

/// Ensure manual partition exists.
fn ensure_manual(storage: &SqliteStorage, initial_id: SnapshotId) {
    layertwine::layered::manual::ensure_manual_partition(storage, initial_id).unwrap();
}

/// Ensure agent partition exists.
fn ensure_agent(storage: &SqliteStorage, agent_id: &AgentInstanceId, initial_id: SnapshotId) {
    layertwine::layered::agent::ensure_agent_partition(storage, agent_id, initial_id).unwrap();
}

/// Ensure approval partition exists.
fn ensure_approval(storage: &SqliteStorage, agent_id: &AgentInstanceId, initial_id: SnapshotId) {
    layertwine::layered::approval::ensure_approval_agent_partition(storage, agent_id, initial_id)
        .unwrap();
}

/// Ensure integrated partition exists.
fn ensure_integrated(storage: &SqliteStorage, name: &str, initial_id: SnapshotId) {
    layertwine::layered::integrated::ensure_integrated_partition(storage, name, initial_id)
        .unwrap();
}

/// Read the current staged snapshot text.
fn staged_text(storage: &SqliteStorage) -> String {
    let pid = layertwine::layered::staged::staged_partition_id();
    let part = storage.get_partition(&pid).unwrap();
    get_text(storage, &part.current_snapshot)
}

// ---------------------------------------------------------------------------
// Test: Full manual pipeline
// ---------------------------------------------------------------------------
#[test]
fn test_full_manual_pipeline() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "line1\nline2\nline3\n");

    ensure_staged(&storage, initial_id);
    ensure_manual(&storage, initial_id);

    // Apply manual edit
    layertwine::layered::manual::apply_manual_edit(
        &storage,
        "test.txt",
        "line1\nmodified\nline3\n",
    )
    .unwrap();

    // Forward: Manual → Staged
    let result = execute_forward(&storage, ForwardTransition::ManualToStaged, &[]);
    assert!(result.is_ok());

    let text = staged_text(&storage);
    assert_eq!(text, "line1\nmodified\nline3\n");
}

// ---------------------------------------------------------------------------
// Test: Full agent pipeline (single agent, single feature)
// ---------------------------------------------------------------------------
#[test]
fn test_full_agent_pipeline() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "base\n");

    ensure_staged(&storage, initial_id);

    let agent_id = AgentInstanceId("agent-1".into());
    let feature = "feat-1";

    ensure_agent(&storage, &agent_id, initial_id);
    ensure_approval(&storage, &agent_id, initial_id);
    ensure_integrated(&storage, feature, initial_id);

    // Agent edits
    layertwine::layered::agent::apply_agent_edit(
        &storage,
        &agent_id,
        "test.txt",
        "base\nagent-change\n",
    )
    .unwrap();

    // Forward: Agent → Approval
    execute_forward(&storage, ForwardTransition::AgentToApproval, &["agent-1"]).unwrap();

    // Forward: Approval → Integrated
    execute_forward(
        &storage,
        ForwardTransition::ApprovalToIntegrated,
        &["agent-1", feature],
    )
    .unwrap();

    // Forward: Integrated → Staged (no unified intermediary)
    execute_forward(&storage, ForwardTransition::IntegratedToStaged, &[feature]).unwrap();

    let text = staged_text(&storage);
    assert_eq!(text, "base\nagent-change\n");
}

// ---------------------------------------------------------------------------
// Test: Multi-agent collaboration on one feature
// ---------------------------------------------------------------------------
#[test]
fn test_multi_agent_collaboration() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "lineA\nlineB\nlineC\n");

    ensure_staged(&storage, initial_id);

    let agent_a = AgentInstanceId("agent-a".into());
    let agent_b = AgentInstanceId("agent-b".into());
    let feature = "collab";

    // Set up both agents
    for agent in [&agent_a, &agent_b] {
        ensure_agent(&storage, agent, initial_id);
        ensure_approval(&storage, agent, initial_id);
    }
    ensure_integrated(&storage, feature, initial_id);

    // Agent A: modify lineB
    layertwine::layered::agent::apply_agent_edit(
        &storage,
        &agent_a,
        "test.txt",
        "lineA\nmodified-by-A\nlineC\n",
    )
    .unwrap();
    execute_forward(&storage, ForwardTransition::AgentToApproval, &["agent-a"]).unwrap();
    execute_forward(
        &storage,
        ForwardTransition::ApprovalToIntegrated,
        &["agent-a", feature],
    )
    .unwrap();

    // Agent B: modify lineC
    layertwine::layered::agent::apply_agent_edit(
        &storage,
        &agent_b,
        "test.txt",
        "lineA\nlineB\nmodified-by-B\n",
    )
    .unwrap();
    execute_forward(&storage, ForwardTransition::AgentToApproval, &["agent-b"]).unwrap();
    execute_forward(
        &storage,
        ForwardTransition::ApprovalToIntegrated,
        &["agent-b", feature],
    )
    .unwrap();

    // Merge feature directly to staged
    execute_forward(&storage, ForwardTransition::IntegratedToStaged, &[feature]).unwrap();

    let text = staged_text(&storage);
    // Both modifications should be present (different lines → no conflict)
    assert!(
        text.contains("modified-by-A"),
        "should contain agent A's change"
    );
    assert!(
        text.contains("modified-by-B"),
        "should contain agent B's change"
    );
    assert!(!text.contains("lineB"), "lineB should be replaced");
    assert!(!text.contains("lineC"), "lineC should be replaced");
}

// ---------------------------------------------------------------------------
// Test: Multi-feature merge into staged (non-overlapping changes)
// ---------------------------------------------------------------------------
#[test]
fn test_multi_feature_merge() {
    let storage = setup_storage();
    // Base has two lines so each feature can modify a different line
    let initial_id = create_initial_snapshot(&storage, "line1\nline2\n");

    ensure_staged(&storage, initial_id);

    let agent1 = AgentInstanceId("agent-1".into());
    let agent2 = AgentInstanceId("agent-2".into());
    let feat1 = "feat-alpha";
    let feat2 = "feat-beta";

    for agent in [&agent1, &agent2] {
        ensure_agent(&storage, agent, initial_id);
        ensure_approval(&storage, agent, initial_id);
    }
    ensure_integrated(&storage, feat1, initial_id);
    ensure_integrated(&storage, feat2, initial_id);

    // Feature 1: modify first line
    layertwine::layered::agent::apply_agent_edit(&storage, &agent1, "test.txt", "alpha\nline2\n")
        .unwrap();
    execute_forward(&storage, ForwardTransition::AgentToApproval, &["agent-1"]).unwrap();
    execute_forward(
        &storage,
        ForwardTransition::ApprovalToIntegrated,
        &["agent-1", feat1],
    )
    .unwrap();

    // Feature 2: modify second line (different from Feature 1)
    layertwine::layered::agent::apply_agent_edit(&storage, &agent2, "test.txt", "line1\nbeta\n")
        .unwrap();
    execute_forward(&storage, ForwardTransition::AgentToApproval, &["agent-2"]).unwrap();
    execute_forward(
        &storage,
        ForwardTransition::ApprovalToIntegrated,
        &["agent-2", feat2],
    )
    .unwrap();

    // Merge both features directly to staged — each modifies a different line → no conflict
    let names = &[feat1.to_string(), feat2.to_string()];
    let merge_result =
        layertwine::layered::staged::merge_features_to_staged(&storage, names).unwrap();
    assert!(
        !merge_result.has_conflicts(),
        "non-overlapping edits should not conflict"
    );

    let text = staged_text(&storage);
    // Features merged sequentially. feat1 changes line1 ("alpha\nline2"),
    // feat2 changes line2 ("line1\nbeta"). When merged sequentially,
    // different-line edits should merge cleanly.
    assert!(
        text.contains("alpha") && text.contains("beta"),
        "both features should be present; got: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// Test: Rollback from staged to manual
// ---------------------------------------------------------------------------
#[test]
fn test_rollback_staged_to_manual() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "original\n");

    ensure_staged(&storage, initial_id);
    ensure_manual(&storage, initial_id);

    // Apply manual edit and merge to staged
    layertwine::layered::manual::apply_manual_edit(&storage, "test.txt", "original\nedited\n")
        .unwrap();
    execute_forward(&storage, ForwardTransition::ManualToStaged, &[]).unwrap();

    // Verify staged has the new content
    let staged_before = staged_text(&storage);
    assert_eq!(staged_before, "original\nedited\n");

    // Rollback staged → manual
    let result = execute_rollback(&storage, RollbackTransition::StagedToManual, &[]);
    assert!(result.is_ok());

    // After rollback, staged should point to the manual partition's snapshot.
    // The manual partition still has its "edited" snapshot, so staged will reflect that.
    let staged_after = staged_text(&storage);
    assert!(
        staged_after.contains("edited"),
        "should still contain manual edits"
    );
}

// ---------------------------------------------------------------------------
// Test: Conflict detection during integrated merge
// ---------------------------------------------------------------------------
#[test]
fn test_merge_conflict_detection() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "shared\ncontent\n");

    let agent_a = AgentInstanceId("agent-a".into());
    let agent_b = AgentInstanceId("agent-b".into());
    let feature = "conflict-feat";

    for agent in [&agent_a, &agent_b] {
        ensure_agent(&storage, agent, initial_id);
        ensure_approval(&storage, agent, initial_id);
    }
    ensure_integrated(&storage, feature, initial_id);

    // Agent A: modify first line
    layertwine::layered::agent::apply_agent_edit(
        &storage,
        &agent_a,
        "test.txt",
        "modified-by-A\ncontent\n",
    )
    .unwrap();
    execute_forward(&storage, ForwardTransition::AgentToApproval, &["agent-a"]).unwrap();
    let r1 = layertwine::layered::integrated::merge_agent_to_feature(&storage, &agent_a, feature)
        .unwrap();
    assert!(!r1.has_conflicts(), "first merge should succeed cleanly");

    // Agent B: modify same first line (different change)
    layertwine::layered::agent::apply_agent_edit(
        &storage,
        &agent_b,
        "test.txt",
        "modified-by-B\ncontent\n",
    )
    .unwrap();
    execute_forward(&storage, ForwardTransition::AgentToApproval, &["agent-b"]).unwrap();
    let r2 = layertwine::layered::integrated::merge_agent_to_feature(&storage, &agent_b, feature)
        .unwrap();

    // Both agents modified the same line → expect conflict
    assert!(
        r2.has_conflicts(),
        "should detect conflict when two agents modify same line"
    );
}

// ---------------------------------------------------------------------------
// Test: No-op forward flow (no changes since last merge)
// ---------------------------------------------------------------------------
#[test]
fn test_idempotent_merge() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "content\n");

    ensure_staged(&storage, initial_id);
    ensure_manual(&storage, initial_id);

    // Merge without any manual edit → should be a no-op
    let result = execute_forward(&storage, ForwardTransition::ManualToStaged, &[]);
    assert!(result.is_ok());

    // Content should remain unchanged
    let pid = layertwine::layered::staged::staged_partition_id();
    let part = storage.get_partition(&pid).unwrap();
    assert_eq!(
        part.current_snapshot, initial_id,
        "no-change merge should not advance staged"
    );
}

// ---------------------------------------------------------------------------
// Test: End-to-end approval → integrated → staged chain
// (without using execute_forward, direct function calls)
// ---------------------------------------------------------------------------
#[test]
fn test_approval_integrated_staged_chain() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "start\n");

    ensure_staged(&storage, initial_id);

    let agent_id = AgentInstanceId("chain-agent".into());
    let feature = "chain-feat";

    ensure_agent(&storage, &agent_id, initial_id);
    ensure_approval(&storage, &agent_id, initial_id);
    ensure_integrated(&storage, feature, initial_id);

    // Apply edit and move to approval
    layertwine::layered::agent::apply_agent_edit(
        &storage,
        &agent_id,
        "test.txt",
        "start\nchained\n",
    )
    .unwrap();
    layertwine::layered::agent::move_agent_to_approval(&storage, &agent_id).unwrap();

    // Direct function call: approval → integrated
    let merge_result =
        layertwine::layered::integrated::merge_agent_to_feature(&storage, &agent_id, feature)
            .unwrap();
    assert!(!merge_result.has_conflicts());
    let integrated_sid = merge_result.snapshot_id;

    // Direct function call: integrated → staged (no unified intermediary)
    let staged_result =
        layertwine::layered::staged::merge_features_to_staged(&storage, &[feature.to_string()])
            .unwrap();

    // Staged snapshot should be different from integrated snapshot
    assert_ne!(staged_result.snapshot_id, integrated_sid, "staged snapshot must advance");

    let text = staged_text(&storage);
    assert_eq!(text, "start\nchained\n");
}

// ---------------------------------------------------------------------------
// Test: Multiple sequential manual edits (without forward between edits)
// ---------------------------------------------------------------------------
#[test]
fn test_sequential_manual_edits() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "line1\nline2\n");

    ensure_manual(&storage, initial_id);

    // Multiple edits without forward in between
    let id1 =
        layertwine::layered::manual::apply_manual_edit(&storage, "test.txt", "line1\nmodified\n")
            .unwrap();
    assert_ne!(id1, initial_id);

    let id2 = layertwine::layered::manual::apply_manual_edit(
        &storage,
        "test.txt",
        "line1\nmodified\nline3\n",
    )
    .unwrap();
    assert_ne!(id2, id1);

    // Now forward to staged in one step
    ensure_staged(&storage, initial_id);
    execute_forward(&storage, ForwardTransition::ManualToStaged, &[]).unwrap();
    assert_eq!(staged_text(&storage), "line1\nmodified\nline3\n");
}

// ---------------------------------------------------------------------------
// Test: Forward → edit again → forward again
// ---------------------------------------------------------------------------
#[test]
fn test_edit_after_forward() {
    let storage = setup_storage();
    let initial_id = create_initial_snapshot(&storage, "base\n");

    ensure_staged(&storage, initial_id);
    ensure_manual(&storage, initial_id);

    layertwine::layered::manual::apply_manual_edit(&storage, "test.txt", "base\na\n").unwrap();
    execute_forward(&storage, ForwardTransition::ManualToStaged, &[]).unwrap();
    assert_eq!(staged_text(&storage), "base\na\n");

    layertwine::layered::manual::apply_manual_edit(&storage, "test.txt", "base\na\nb\n").unwrap();
    execute_forward(&storage, ForwardTransition::ManualToStaged, &[]).unwrap();
    assert_eq!(staged_text(&storage), "base\na\nb\n");
}

// ---------------------------------------------------------------------------
// Test: State machine integration (StateMachine wrapper)
// ---------------------------------------------------------------------------
#[test]
fn test_state_machine_integration() {
    use layertwine::layered::StateMachine;

    let storage = Arc::new(setup_storage());
    let sm = StateMachine::new(storage.clone());
    let s: &SqliteStorage = &storage; // deref Arc for non-Arc calls

    // Create initial snapshot and partitions
    let initial_id = {
        let file_node = FileNode::new(PathBuf::from("test.txt"), b"sm\n");
        s.store_file_node(&file_node, b"sm\n").unwrap();
        let empty_diff = LineDiff::new(vec![]);
        let delta = Delta::new(file_node.clone(), empty_diff, SourceType::Manual);
        s.store_delta(&delta).unwrap();
        let snapshot = Snapshot::new_initial(file_node, delta.id);
        s.store_snapshot(&snapshot, b"").unwrap();
        snapshot.id
    };

    ensure_staged(s, initial_id);
    ensure_manual(s, initial_id);

    // Apply manual edit
    layertwine::layered::manual::apply_manual_edit(s, "test.txt", "sm\nedited\n").unwrap();

    // Execute forward via state machine
    let sid = sm
        .with_transaction(|s| execute_forward(s, ForwardTransition::ManualToStaged, &[]))
        .unwrap();

    // Verify via sm
    let staged_pid = layertwine::layered::staged::staged_partition_id();
    let part = sm
        .get_partition(&staged_pid)
        .unwrap();
    assert_eq!(part.current_snapshot, sid);

    // Sync layers
    sm.sync_layers().unwrap();
}

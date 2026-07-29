//! Integration tests for the checkpoint module.
//!
//! These tests exercise the checkpoint repository functionality end-to-end.
//! They verify:
//! - Creating checkpoints with single and multiple snapshots
//! - Branch creation, switching, and merging
//! - DAG operations and ancestor tracking
//! - Persistence and loading from storage
//! - Log history and traversal

use layertwine::checkpoint::branch::Branch;
use layertwine::checkpoint::repo::CheckpointRepo;
use layertwine::checkpoint::restore::RestoreRequest;
use layertwine::checkpoint::types::{Checkpoint, CheckpointBuilder, CheckpointMetadata};
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::{Snapshot, SnapshotContent};
use layertwine::core::types::{CheckpointId, ContentId, SnapshotId};

fn dummy_snapshot_id(n: u8) -> SnapshotId {
    ContentId::from_content(&[n; 8])
}

// ---------------------------------------------------------------------------
// Test: Checkpoint creation and basic operations
// ---------------------------------------------------------------------------

#[test]
fn test_checkpoint_repo_init() {
    let snap = dummy_snapshot_id(1);
    let repo = CheckpointRepo::new_single(snap);

    assert_eq!(repo.branches.len(), 1);
    assert_eq!(repo.branches[0].name, "main");
    assert_eq!(repo.checkpoint_count(), 1);
}

#[test]
fn test_checkpoint_repo_init_multi_snapshot() {
    let snap1 = dummy_snapshot_id(1);
    let snap2 = dummy_snapshot_id(2);
    let snapshots = vec![snap1, snap2];

    let repo = CheckpointRepo::new(snapshots);

    assert_eq!(repo.checkpoint_count(), 1);
    let root = repo.get_checkpoint(&repo.current_branch_head()).unwrap();
    assert_eq!(root.baseline_snapshots.len(), 2);
}

#[test]
fn test_linear_commit_history() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second commit", "user").unwrap();

    let snap3 = dummy_snapshot_id(3);
    let cp2 = repo.commit_single(snap3, "third commit", "user").unwrap();

    let log = repo.log(10);
    assert_eq!(log.len(), 3);
    assert_eq!(log[0].id, cp2);
    assert_eq!(log[1].id, cp1);
    assert_eq!(log[0].parents.len(), 1);
}

#[test]
fn test_multi_snapshot_commit() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let snap3 = dummy_snapshot_id(3);
    let cp1 = repo
        .commit(vec![snap2, snap3], "multi-file commit", "user")
        .unwrap();

    let cp = repo.get_checkpoint(&cp1).unwrap();
    assert_eq!(cp.baseline_snapshots.len(), 2);
    assert!(cp.baseline_snapshots.contains(&snap2));
    assert!(cp.baseline_snapshots.contains(&snap3));
}

#[test]
fn test_empty_commit_fails() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    let result = repo.commit(vec![], "empty commit", "user");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: Branch operations
// ---------------------------------------------------------------------------

#[test]
fn test_create_branch() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    repo.create_branch("feature").unwrap();

    assert_eq!(repo.branches.len(), 2);
    assert!(repo.branches.iter().any(|b| b.name == "feature"));
}

#[test]
fn test_create_branch_from_checkpoint() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second", "user").unwrap();

    repo.create_branch_from("feature", cp1).unwrap();

    assert_eq!(repo.branches.len(), 2);
    let feature_idx = repo.find_branch("feature").unwrap();
    assert_eq!(repo.branches[feature_idx].head, cp1);
}

#[test]
fn test_switch_branch() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    repo.create_branch("feature").unwrap();
    repo.switch_branch("feature").unwrap();

    assert_eq!(repo.current_branch_name(), "feature");
}

#[test]
fn test_create_duplicate_branch_fails() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    repo.create_branch("feature").unwrap();
    let result = repo.create_branch("feature");
    assert!(result.is_err());
}

#[test]
fn test_switch_nonexistent_branch_fails() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    let result = repo.switch_branch("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_get_branch_head() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second", "user").unwrap();

    repo.create_branch("feature").unwrap();
    repo.switch_branch("feature").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "feature commit", "user").unwrap();

    let feature_head = repo.get_branch_head("feature").unwrap();
    let main_head = repo.get_branch_head("main").unwrap();

    assert_eq!(main_head, cp1);
    assert_ne!(feature_head, main_head);
}

// ---------------------------------------------------------------------------
// Test: Merge operations
// ---------------------------------------------------------------------------

#[test]
fn test_merge_branches() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    repo.commit_single(snap2, "main v2", "user").unwrap();

    repo.create_branch("feature").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "feature v1", "user").unwrap();

    repo.switch_branch("main").unwrap();

    let snap4 = dummy_snapshot_id(4);
    let merge_cp = repo
        .merge_branches("feature", vec![snap4], "merge feature", "user")
        .unwrap();

    let cp = repo.get_checkpoint(&merge_cp).unwrap();
    assert_eq!(
        cp.parents.len(),
        2,
        "merge checkpoint should have 2 parents"
    );
    assert_eq!(cp.metadata.message, "merge feature");
}

#[test]
fn test_merge_with_empty_snapshots_fails() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    repo.create_branch("feature").unwrap();
    let result = repo.merge_branches("feature", vec![], "merge", "user");
    assert!(result.is_err());
}

#[test]
fn test_merge_nonexistent_branch_fails() {
    let snap1 = dummy_snapshot_id(1);
    let snap2 = dummy_snapshot_id(2);
    let mut repo = CheckpointRepo::new_single(snap1);

    let result = repo.merge_branches("nonexistent", vec![snap2], "merge", "user");
    assert!(result.is_err());
}

#[test]
fn test_merge_creates_multiple_parents() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let main_cp = repo.commit_single(snap2, "main commit", "user").unwrap();

    repo.create_branch("feature").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "feature commit", "user").unwrap();

    repo.switch_branch("main").unwrap();

    let snap4 = dummy_snapshot_id(4);
    let merge_cp = repo
        .merge_branches("feature", vec![snap4], "merge", "user")
        .unwrap();

    let merge = repo.get_checkpoint(&merge_cp).unwrap();
    assert!(merge.parents.contains(&main_cp));
}

// ---------------------------------------------------------------------------
// Test: Log and history
// ---------------------------------------------------------------------------

#[test]
fn test_log_count() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    for i in 2..=10 {
        repo.commit_single(dummy_snapshot_id(i), &format!("commit {}", i), "user")
            .unwrap();
    }

    let log = repo.log(5);
    assert_eq!(log.len(), 5);
    assert_eq!(log[0].metadata.message, "commit 10");
}

#[test]
fn test_log_from_specific_checkpoint() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second", "user").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "third", "user").unwrap();

    let log = repo.log_from(&cp1, 10);
    assert!(!log.is_empty());
    assert_eq!(log[0].id, cp1);
}

#[test]
fn test_log_from_merge_commit() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    repo.commit_single(snap2, "main v2", "user").unwrap();

    repo.create_branch("feature").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "feature v1", "user").unwrap();

    repo.switch_branch("main").unwrap();

    let snap4 = dummy_snapshot_id(4);
    repo.merge_branches("feature", vec![snap4], "merge", "user")
        .unwrap();

    let log = repo.log(10);
    assert!(log.len() >= 4);

    let merge_cp = &log[0];
    assert_eq!(merge_cp.parents.len(), 2, "merge should have 2 parents");
}

// ---------------------------------------------------------------------------
// Test: DAG operations
// ---------------------------------------------------------------------------

#[test]
fn test_dag_tracking() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second", "user").unwrap();

    let dag = repo.dag();
    assert!(dag.has_node(&repo.current_branch_head()));
    assert!(dag.has_node(&cp1));
    assert_eq!(dag.len(), 2);
}

#[test]
fn test_dag_children() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second", "user").unwrap();

    let dag = repo.dag();
    let log = repo.log(10);
    let root_id = log.iter().find(|cp| cp.parents.is_empty()).unwrap().id;

    let children = dag.get_children(&root_id);

    assert!(children.contains(&cp1));
}

#[test]
fn test_dag_generation() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    repo.commit_single(snap2, "second", "user").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "third", "user").unwrap();

    let dag = repo.dag();
    let log = repo.log(10);

    assert_eq!(dag.generation(&log[0].id), Some(2));
    assert_eq!(dag.generation(&log[1].id), Some(1));
    assert_eq!(dag.generation(&log[2].id), Some(0));
}

#[test]
fn test_dag_ancestor_lookup() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let log = repo.log(10);
    let root_id = log.iter().find(|cp| cp.parents.is_empty()).unwrap().id;

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second", "user").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "third", "user").unwrap();

    let head = repo.current_branch_head();
    let dag = repo.dag();

    let get_parents = |id: &CheckpointId| -> Vec<CheckpointId> {
        repo.get_checkpoint(id)
            .map(|cp| cp.parents.clone())
            .unwrap_or_default()
    };

    let ancestors = dag.ancestors(&head, get_parents);
    assert!(ancestors.contains(&cp1));
    assert!(ancestors.contains(&root_id));
}

#[test]
fn test_dag_is_ancestor() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let root_id = repo.current_branch_head();

    let snap2 = dummy_snapshot_id(2);
    repo.commit_single(snap2, "second", "user").unwrap();

    let head = repo.current_branch_head();
    let dag = repo.dag();

    assert!(dag.is_ancestor(&root_id, &head));
    assert!(!dag.is_ancestor(&head, &root_id));
}

#[test]
fn test_dag_merge_base() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let initial_log = repo.log(10);
    let _root_id = initial_log
        .iter()
        .find(|cp| cp.parents.is_empty())
        .unwrap()
        .id;

    let snap2 = dummy_snapshot_id(2);
    let main_v2 = repo.commit_single(snap2, "main v2", "user").unwrap();

    repo.create_branch("feature").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "feature v1", "user").unwrap();

    repo.switch_branch("main").unwrap();

    let snap4 = dummy_snapshot_id(4);
    repo.commit_single(snap4, "main v3", "user").unwrap();

    let main_head = repo.current_branch_head();
    let feature_head = repo.get_branch_head("feature").unwrap();
    let dag = repo.dag();

    let get_parents = |id: &CheckpointId| -> Vec<CheckpointId> {
        repo.get_checkpoint(id)
            .map(|cp| cp.parents.clone())
            .unwrap_or_default()
    };

    let merge_base = dag.merge_base(&main_head, &feature_head, get_parents);
    assert!(merge_base.is_some(), "should find a merge base");
    assert_eq!(merge_base, Some(main_v2), "merge base should be main_v2");
}

// ---------------------------------------------------------------------------
// Test: Checkpoint deletion
// ---------------------------------------------------------------------------

#[test]
fn test_remove_checkpoint() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let _cp1 = repo.commit_single(snap2, "second", "user").unwrap();

    let snap3 = dummy_snapshot_id(3);
    let cp2 = repo.commit_single(snap3, "third", "user").unwrap();

    repo.remove_checkpoint(&cp2).unwrap();

    assert_eq!(repo.checkpoint_count(), 2);
    assert!(repo.get_checkpoint(&cp2).is_err());
    assert!(!repo.checkpoint_dag.has_node(&cp2));
}

#[test]
fn test_remove_nonexistent_checkpoint_fails() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    let fake_id = CheckpointId::from_content(b"fake");
    let result = repo.remove_checkpoint(&fake_id);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: CheckpointBuilder
// ---------------------------------------------------------------------------

#[test]
fn test_checkpoint_builder() {
    let snap = dummy_snapshot_id(1);
    let parent_id = CheckpointId::from_content(b"parent");

    let cp = CheckpointBuilder::new()
        .baseline_snapshot(snap)
        .author("builder-user")
        .message("built checkpoint")
        .parent(parent_id)
        .build()
        .unwrap();

    assert_eq!(cp.metadata.author, "builder-user");
    assert_eq!(cp.metadata.message, "built checkpoint");
    assert_eq!(cp.baseline_snapshots, vec![snap]);
    assert_eq!(cp.parents, vec![parent_id]);
}

#[test]
fn test_checkpoint_builder_multi_snapshot() {
    let snap1 = dummy_snapshot_id(1);
    let snap2 = dummy_snapshot_id(2);
    let parent_id = CheckpointId::from_content(b"parent");

    let cp = CheckpointBuilder::new()
        .baseline_snapshots(vec![snap1, snap2])
        .author("multi")
        .message("multi snapshot commit")
        .parent(parent_id)
        .build()
        .unwrap();

    assert_eq!(cp.baseline_snapshots.len(), 2);
    assert!(cp.baseline_snapshots.contains(&snap1));
    assert!(cp.baseline_snapshots.contains(&snap2));
}

#[test]
fn test_checkpoint_builder_empty_snapshots_fails() {
    let result = CheckpointBuilder::new()
        .author("user")
        .message("no snapshots")
        .build();

    assert!(result.is_err());
}

#[test]
fn test_checkpoint_builder_with_git_anchor() {
    let snap = dummy_snapshot_id(1);

    let cp = CheckpointBuilder::new()
        .baseline_snapshot(snap)
        .author("user")
        .message("message")
        .git_anchor("abc123")
        .build()
        .unwrap();

    assert_eq!(cp.metadata.git_anchor, Some("abc123".to_string()));
}

#[test]
fn test_checkpoint_builder_multiple_parents() {
    let snap = dummy_snapshot_id(1);
    let parent1 = CheckpointId::from_content(b"parent1");
    let parent2 = CheckpointId::from_content(b"parent2");

    let cp = CheckpointBuilder::new()
        .baseline_snapshot(snap)
        .parents(vec![parent1, parent2])
        .author("user")
        .message("merge")
        .build()
        .unwrap();

    assert_eq!(cp.parents.len(), 2);
    assert!(cp.parents.contains(&parent1));
    assert!(cp.parents.contains(&parent2));
}

// ---------------------------------------------------------------------------
// Test: Checkpoint content addressing
// ---------------------------------------------------------------------------

#[test]
fn test_checkpoint_content_addressing() {
    let snap = dummy_snapshot_id(1);
    let cp1 = Checkpoint::new_single(snap, vec![], CheckpointMetadata::new("user", "message"));
    let cp2 = Checkpoint::new_single(snap, vec![], CheckpointMetadata::new("user", "message"));

    assert_eq!(cp1.id, cp2.id, "same content = same id");

    let cp3 = Checkpoint::new_single(snap, vec![], CheckpointMetadata::new("other", "message"));
    assert_ne!(cp1.id, cp3.id, "different author = different id");
}

#[test]
fn test_checkpoint_id_excludes_created_at() {
    let snap = dummy_snapshot_id(1);
    let metadata = CheckpointMetadata::new("user", "message");

    let cp1 = Checkpoint::new_single(snap, vec![], metadata.clone());

    std::thread::sleep(std::time::Duration::from_millis(10));

    let cp2 = Checkpoint::new_single(snap, vec![], metadata);

    assert_eq!(
        cp1.id, cp2.id,
        "ID should be same despite different timestamps"
    );
    assert_ne!(
        cp1.created_at, cp2.created_at,
        "Timestamps should be different"
    );
}

#[test]
fn test_checkpoint_id_excludes_git_anchor() {
    let snap = dummy_snapshot_id(1);
    let mut metadata1 = CheckpointMetadata::new("user", "message");
    metadata1.git_anchor = Some("commit1".to_string());

    let mut metadata2 = CheckpointMetadata::new("user", "message");
    metadata2.git_anchor = Some("commit2".to_string());

    let cp1 = Checkpoint::new_single(snap, vec![], metadata1);
    let cp2 = Checkpoint::new_single(snap, vec![], metadata2);

    assert_eq!(cp1.id, cp2.id, "ID should be same regardless of git_anchor");
}

// ---------------------------------------------------------------------------
// Test: Branch entity operations
// ---------------------------------------------------------------------------

#[test]
fn test_branch_creation() {
    let head = dummy_checkpoint_id();
    let branch = Branch::new("main", head);

    assert_eq!(branch.name, "main");
    assert_eq!(branch.head, head);
}

#[test]
fn test_branch_set_head() {
    let head1 = dummy_checkpoint_id();
    let head2 = CheckpointId::from_content(b"new-checkpoint");
    let mut branch = Branch::new("feature", head1);

    branch.set_head(head2);

    assert_eq!(branch.head, head2);
}

fn dummy_checkpoint_id() -> CheckpointId {
    CheckpointId::from_content(b"test-checkpoint")
}

// ---------------------------------------------------------------------------
// Test: Multiple branch switching
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_branch_switching() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    repo.create_branch("feature-a").unwrap();
    repo.create_branch("feature-b").unwrap();

    repo.switch_branch("feature-a").unwrap();
    assert_eq!(repo.current_branch_name(), "feature-a");

    repo.switch_branch("feature-b").unwrap();
    assert_eq!(repo.current_branch_name(), "feature-b");

    repo.switch_branch("main").unwrap();
    assert_eq!(repo.current_branch_name(), "main");
}

// ---------------------------------------------------------------------------
// Test: Complex merge scenarios
// ---------------------------------------------------------------------------

#[test]
fn test_complex_merge_scenario() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    repo.commit_single(snap2, "main v2", "user").unwrap();

    repo.create_branch("feature-a").unwrap();
    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "feature-a commit", "user")
        .unwrap();

    repo.switch_branch("main").unwrap();
    repo.create_branch("feature-b").unwrap();
    let snap4 = dummy_snapshot_id(4);
    repo.commit_single(snap4, "feature-b commit", "user")
        .unwrap();

    repo.switch_branch("feature-a").unwrap();

    let snap5 = dummy_snapshot_id(5);
    repo.merge_branches("feature-b", vec![snap5], "merge a and b", "user")
        .unwrap();

    let head = repo.current_branch_head();
    let cp = repo.get_checkpoint(&head).unwrap();
    assert_eq!(cp.parents.len(), 2);
}

// ---------------------------------------------------------------------------
// Test: List branches
// ---------------------------------------------------------------------------

#[test]
fn test_list_branches() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    repo.create_branch("feature-a").unwrap();
    repo.create_branch("feature-b").unwrap();

    let branches = repo.list_branches();
    assert_eq!(branches.len(), 3);
}

// ---------------------------------------------------------------------------
// Test: Find branch
// ---------------------------------------------------------------------------

#[test]
fn test_find_branch() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    repo.create_branch("feature").unwrap();
    let idx = repo.find_branch("feature").unwrap();

    assert_eq!(repo.branches[idx].name, "feature");

    let result = repo.find_branch("nonexistent");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: Rollback to checkpoint
// ---------------------------------------------------------------------------

#[test]
fn test_rollback_to_checkpoint() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second commit", "user").unwrap();

    let snap3 = dummy_snapshot_id(3);
    repo.commit_single(snap3, "third commit", "user").unwrap();

    // Rollback to second commit
    let snapshots = repo.rollback_to(&cp1).unwrap();
    assert_eq!(snapshots, vec![snap2]);
}

#[test]
fn test_rollback_to_multi_file_checkpoint() {
    let snap1 = dummy_snapshot_id(1);
    let snap2 = dummy_snapshot_id(2);
    let mut repo = CheckpointRepo::new(vec![snap1, snap2]);

    let snap3 = dummy_snapshot_id(3);
    let snap4 = dummy_snapshot_id(4);
    let cp1 = repo
        .commit(vec![snap3, snap4], "multi-file commit", "user")
        .unwrap();

    let snap5 = dummy_snapshot_id(5);
    repo.commit_single(snap5, "another commit", "user").unwrap();

    // Rollback to multi-file commit
    let snapshots = repo.rollback_to(&cp1).unwrap();
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots.contains(&snap3));
    assert!(snapshots.contains(&snap4));
}

#[test]
fn test_rollback_to_nonexistent_fails() {
    let snap = dummy_snapshot_id(1);
    let repo = CheckpointRepo::new_single(snap);

    let fake_id = CheckpointId::from_content(b"fake");
    let result = repo.rollback_to(&fake_id);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: Get ancestors to checkpoint
// ---------------------------------------------------------------------------

#[test]
fn test_get_ancestors_to() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second commit", "user").unwrap();

    let snap3 = dummy_snapshot_id(3);
    let cp2 = repo.commit_single(snap3, "third commit", "user").unwrap();

    let snap4 = dummy_snapshot_id(4);
    let cp3 = repo.commit_single(snap4, "fourth commit", "user").unwrap();

    // Get ancestors from current head to cp1
    let ancestors = repo.get_ancestors_to(&cp1).unwrap();
    assert_eq!(ancestors.len(), 3);
    assert_eq!(ancestors[0].id, cp3);
    assert_eq!(ancestors[1].id, cp2);
    assert_eq!(ancestors[2].id, cp1);
}

#[test]
fn test_get_ancestors_to_non_ancestor_fails() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    repo.create_branch("feature").unwrap();

    let snap2 = dummy_snapshot_id(2);
    repo.commit_single(snap2, "main commit", "user").unwrap();

    repo.switch_branch("feature").unwrap();

    let snap3 = dummy_snapshot_id(3);
    let feature_cp = repo.commit_single(snap3, "feature commit", "user").unwrap();

    repo.switch_branch("main").unwrap();

    // Try to get ancestors to feature checkpoint (not on current branch)
    let result = repo.get_ancestors_to(&feature_cp);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: DAG built dynamically from checkpoints
// ---------------------------------------------------------------------------

#[test]
fn test_dag_built_dynamically() {
    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    let cp1 = repo.commit_single(snap2, "second commit", "user").unwrap();

    let snap3 = dummy_snapshot_id(3);
    let cp2 = repo.commit_single(snap3, "third commit", "user").unwrap();

    // Verify DAG was built correctly
    let dag = repo.dag();
    assert!(dag.has_node(&repo.current_branch_head()));
    assert!(dag.is_ancestor(&cp1, &cp2));
    assert_eq!(dag.get_children(&cp1), vec![cp2]);
}

#[test]
fn test_dag_built_after_load() {
    use layertwine::storage::SqliteStorage;

    let storage = SqliteStorage::new_full_in_memory().unwrap();

    let snap1 = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap1);

    let snap2 = dummy_snapshot_id(2);
    repo.commit_single(snap2, "second commit", "user").unwrap();

    // Sync to storage
    repo.attach_storage(Box::new(storage.clone()));
    repo.sync_all().unwrap();

    // Load from storage (DAG should be rebuilt dynamically)
    let loaded_repo = CheckpointRepo::load(Box::new(storage)).unwrap();

    // Verify DAG was rebuilt correctly
    let head = loaded_repo.current_branch_head();
    assert!(loaded_repo.dag().has_node(&head));
    assert_eq!(loaded_repo.checkpoint_count(), 2);
}

// ---------------------------------------------------------------------------
// Test: Restore operations
// ---------------------------------------------------------------------------

/// Create a snapshot with content-addressed ID and cache it in the repo.
fn make_cached_snapshot(seed: u8, source: &str) -> Snapshot {
    let file = FileNode::new("dummy".into(), &[seed]);
    Snapshot::new_with_content(
        file,
        SnapshotContent::FileContent(vec![seed]),
        source.to_string(),
        String::new(),
        vec![],
        vec![],
    )
}

/// Build a repo whose root checkpoint carries multiple pre-cached snapshots.
fn multi_snapshot_repo(specs: Vec<(u8, &str)>) -> CheckpointRepo {
    let snaps: Vec<Snapshot> = specs
        .iter()
        .map(|&(s, src)| make_cached_snapshot(s, src))
        .collect();
    let ids: Vec<SnapshotId> = snaps.iter().map(|sn| sn.id).collect();
    let mut repo = CheckpointRepo::new(ids);
    let root_id = repo.current_branch_head();
    for sn in &snaps {
        let _ = repo.set_snapshot_source(&root_id, sn.id, sn.source.clone());
    }
    for sn in snaps {
        repo.cache_snapshot(sn);
    }
    repo
}

#[test]
fn test_restore_full_integration() {
    let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs"), (2, "agent://state")]);

    let head = repo.current_branch_head();
    let resp = repo.restore_full(&head).unwrap();

    assert_eq!(resp.snapshots.len(), 2);
    assert!(!resp.ancestry.is_empty());
    assert_eq!(*resp.ancestry.last().unwrap(), head);
}

#[test]
fn test_restore_selective_integration() {
    let repo = multi_snapshot_repo(vec![
        (1, "file://src/main.rs"),
        (2, "agent://state"),
        (3, "graph://exec"),
    ]);

    let head = repo.current_branch_head();
    let resp = repo
        .restore_selective(&head, vec!["agent://", "graph://"])
        .unwrap();

    assert_eq!(resp.snapshots.len(), 2);
    let sources: Vec<&str> = resp.snapshots.iter().map(|(_, _, s)| s.as_str()).collect();
    assert!(sources.contains(&"agent://state"));
    assert!(sources.contains(&"graph://exec"));
}

#[test]
fn test_restore_dispatcher_integration() {
    let repo = multi_snapshot_repo(vec![(1, "file://src/a.rs"), (2, "agent://state")]);

    let head = repo.current_branch_head();

    // checkpoint_id only → full restore
    let req = RestoreRequest {
        checkpoint_id: Some(head),
        source_filter: None,
        time_range: None,
    };
    let resp = repo.restore(&req).unwrap();
    assert_eq!(resp.snapshots.len(), 2);

    // checkpoint_id + filter → selective
    let req = RestoreRequest {
        checkpoint_id: Some(head),
        source_filter: Some(vec!["agent://".to_string()]),
        time_range: None,
    };
    let resp = repo.restore(&req).unwrap();
    assert_eq!(resp.snapshots.len(), 1);
    assert_eq!(resp.snapshots[0].2, "agent://state");

    // neither → error
    let req = RestoreRequest {
        checkpoint_id: None,
        source_filter: None,
        time_range: None,
    };
    assert!(repo.restore(&req).is_err());
}

#[test]
fn test_diff_checkpoints_integration() {
    let repo = multi_snapshot_repo(vec![(1, "file://a.rs"), (2, "file://b.rs")]);

    let snap = make_cached_snapshot(3, "file://c.rs");
    let snap_id = snap.id;
    let mut repo2 = repo;
    repo2.cache_snapshot(snap);

    let root_id = repo2.current_branch_head();
    repo2.commit_single(snap_id, "add c", "test").unwrap();
    let head = repo2.current_branch_head();
    let _ = repo2.set_snapshot_source(&head, snap_id, "file://c.rs".to_string());

    let diff = repo2.diff_checkpoints(&root_id, &head).unwrap();
    // root had [a, b], head has [c] → a, b removed, c added
    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.removed.len(), 2);
}

#[test]
fn test_validate_integrity_integration() {
    let repo = multi_snapshot_repo(vec![(1, "file://src/main.rs")]);
    let head = repo.current_branch_head();
    let issues = repo.validate_integrity(&head).unwrap();
    assert!(
        issues.is_empty(),
        "expected clean integrity, got: {:?}",
        issues
    );
}

// ---------------------------------------------------------------------------
// Complex DAG: 5+ branches with interleaved merges
// ---------------------------------------------------------------------------

#[test]
fn test_complex_dag_multi_branch_merge() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    // Create 3 feature branches from root
    repo.create_branch("feature-a").unwrap();
    repo.create_branch("feature-b").unwrap();
    repo.create_branch("feature-c").unwrap();

    // Commit on feature-a
    repo.switch_branch("feature-a").unwrap();
    let snap_a1 = dummy_snapshot_id(10);
    let cp_a1 = repo.commit_single(snap_a1, "feature-a v1", "dev").unwrap();

    // Commit on feature-b
    repo.switch_branch("feature-b").unwrap();
    let snap_b1 = dummy_snapshot_id(20);
    let cp_b1 = repo.commit_single(snap_b1, "feature-b v1", "dev").unwrap();
    let snap_b2 = dummy_snapshot_id(21);
    let _cp_b2 = repo.commit_single(snap_b2, "feature-b v2", "dev").unwrap();

    // Commit on feature-c
    repo.switch_branch("feature-c").unwrap();
    let snap_c1 = dummy_snapshot_id(30);
    let _cp_c1 = repo.commit_single(snap_c1, "feature-c v1", "dev").unwrap();

    // Merge feature-a into feature-c
    let snap_m1 = dummy_snapshot_id(40);
    let merge_cp = repo
        .merge_branches("feature-a", vec![snap_m1], "merge a into c", "dev")
        .unwrap();

    let cp = repo.get_checkpoint(&merge_cp).unwrap();
    assert_eq!(cp.parents.len(), 2, "merge should have 2 parents");

    // Verify DAG knows about all branches
    let branches = repo.list_branches();
    assert_eq!(branches.len(), 4, "root + 3 feature branches = 4");

    // Verify feature-b is independent (no merge_base with feature-c after merge)
    let dag = repo.dag();
    let feature_b_head = repo.get_branch_head("feature-b").unwrap();
    let feature_c_head = repo.get_branch_head("feature-c").unwrap();

    let get_parents = |id: &CheckpointId| -> Vec<CheckpointId> {
        repo.get_checkpoint(id)
            .map(|cp| cp.parents.clone())
            .unwrap_or_default()
    };

    // feature-b and feature-c should have a merge-base at root
    let mb = dag.merge_base(&feature_b_head, &feature_c_head, get_parents);
    assert!(mb.is_some(), "branches should have a merge base");
}

// ---------------------------------------------------------------------------
// Complex DAG: diamond merge (two branches merged into main independently)
// ---------------------------------------------------------------------------

#[test]
fn test_diamond_merge_dag() {
    let snap = dummy_snapshot_id(1);
    let mut repo = CheckpointRepo::new_single(snap);

    // Main branch: commit v2
    let snap_m2 = dummy_snapshot_id(2);
    let cp_m2 = repo.commit_single(snap_m2, "main v2", "dev").unwrap();

    // Branch A from main v2
    repo.create_branch("branch-a").unwrap();
    let snap_a1 = dummy_snapshot_id(10);
    let _cp_a1 = repo.commit_single(snap_a1, "branch-a v1", "dev").unwrap();

    // Branch B from main v2
    repo.create_branch("branch-b").unwrap();
    let snap_b1 = dummy_snapshot_id(20);
    let _cp_b1 = repo.commit_single(snap_b1, "branch-b v1", "dev").unwrap();

    // Merge A into main
    repo.switch_branch("main").unwrap();
    let snap_m3 = dummy_snapshot_id(3);
    let merge_a = repo
        .merge_branches("branch-a", vec![snap_m3], "merge a", "dev")
        .unwrap();

    // Merge B into main (now main has both A and B)
    let snap_m4 = dummy_snapshot_id(4);
    let merge_b = repo
        .merge_branches("branch-b", vec![snap_m4], "merge b", "dev")
        .unwrap();

    let head = repo.current_branch_head();

    // DAG should show main_v2 as ancestor of both merge commits
    let dag = repo.dag();
    assert!(dag.is_ancestor(&cp_m2, &merge_a));
    assert!(dag.is_ancestor(&cp_m2, &merge_b));
    assert!(dag.is_ancestor(&merge_a, &head));
    assert!(dag.is_ancestor(&merge_b, &head));
}


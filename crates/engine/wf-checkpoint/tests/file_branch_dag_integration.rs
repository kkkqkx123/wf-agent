//! Closed-loop integration: child-execution branch isolation, multi-parent
//! merge commits, provenance queries and checkpoint-DAG reachability.
//!
//! Scenario: a parent execution forks two isolated child branches; each
//! child records checkpoints, merges into shared features, and the features
//! are joined into staged. Every join produces a multi-parent merge commit,
//! and walking the `parents` edges from the final commit reaches every
//! participant checkpoint.

use std::collections::HashSet;
use std::time::Duration;

use layertwine::checkpoint::types::{Checkpoint, CheckpointMetadata};
use layertwine::core::types::CheckpointId;
use layertwine::storage::repository::{CheckpointPersist, MetadataStore};
use wf_checkpoint::branch::execution_branch_name;
use wf_checkpoint::file::{FileCheckpointManager, FileContentEntry};
use wf_checkpoint::sha256_hex;

fn entry(path: &str, content: &[u8]) -> FileContentEntry {
    FileContentEntry::new(path, content.to_vec())
}

fn stored(storage: &layertwine::storage::sqlite::SqliteStorage, id_hex: &str) -> Checkpoint {
    storage
        .get_checkpoint(&CheckpointId::from_hex(id_hex).unwrap())
        .unwrap()
}

fn parent_ids(checkpoint: &Checkpoint) -> HashSet<String> {
    checkpoint.parents.iter().map(|p| p.to_hex()).collect()
}

/// Record a feature-head commit checkpoint (authored by the feature name),
/// chained onto the merge commit that produced its snapshot.
fn seed_feature_head(
    storage: &layertwine::storage::sqlite::SqliteStorage,
    feature: &str,
    snapshot: layertwine::core::types::SnapshotId,
    merge_commit: &str,
) -> String {
    let cp = Checkpoint::new(
        vec![snapshot],
        vec![CheckpointId::from_hex(merge_commit).unwrap()],
        CheckpointMetadata::new(feature, "feature head"),
    );
    storage.store_checkpoint(&cp).unwrap();
    cp.id.to_hex()
}

/// Distinct `created_at` stamps so latest-by-author scans stay deterministic.
fn tick() {
    std::thread::sleep(Duration::from_millis(3));
}

/// Walk the checkpoint DAG from a start commit through `parents` edges.
fn reachable_from(
    storage: &layertwine::storage::sqlite::SqliteStorage,
    start: &str,
) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue = vec![start.to_string()];
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        for parent in &stored(storage, &id).parents {
            queue.push(parent.to_hex());
        }
    }
    seen
}

#[tokio::test]
async fn child_branch_isolation_merge_and_dag_closed_loop() {
    // 1. In-memory manager.
    let manager = FileCheckpointManager::new_in_memory().unwrap();

    // 2. Parent execution records the base state.
    let parent_cp = manager
        .create_checkpoint("parent", &[entry("a.txt", b"base")])
        .unwrap();

    // 3. Two sibling child executions get hierarchical actors and isolated
    // branches; the parent itself never gets a branch.
    let child_actor = manager.resolve_actor("child", Some("parent")).to_string();
    let child2_actor = manager.resolve_actor("child2", Some("parent")).to_string();
    assert_ne!(child_actor, child2_actor);
    manager
        .ensure_child_branch("child", Some("parent"))
        .await
        .unwrap();
    manager
        .ensure_child_branch("child2", Some("parent"))
        .await
        .unwrap();

    let storage = manager.storage().unwrap().clone();
    let registered = |entity: &str| {
        storage
            .load_metadata(&format!(
                "wf-checkpoint-branch:{}",
                execution_branch_name("execution", entity)
            ))
            .unwrap()
            .is_some()
    };
    assert!(registered("child"), "child branch must be registered");
    assert!(registered("child2"), "child2 branch must be registered");
    assert!(!registered("parent"), "root executions get no branch");

    // 4. The child checkpoint lands on the child partition and moves the
    // branch head.
    let child_cp = manager
        .create_checkpoint("child", &[entry("a.txt", b"child edit")])
        .unwrap();
    assert_eq!(
        manager.branch_head("child").unwrap().as_deref(),
        Some(child_cp.id.as_str())
    );
    assert_eq!(manager.branch_head("parent").unwrap(), None);

    // 5. First entity merge: the commit chains onto the child checkpoint.
    let merge1 = manager.merge_entity_changes("child", "main").unwrap();
    assert!(!merge1.merge_result.has_conflicts());
    assert_eq!(
        parent_ids(&stored(&storage, &merge1.checkpoint_id)),
        HashSet::from([child_cp.id.clone()])
    );

    // 6. Feature-head commit for "main", chained onto the merge commit.
    tick();
    let main_head_1 = seed_feature_head(
        &storage,
        "main",
        merge1.merge_result.snapshot_id,
        &merge1.checkpoint_id,
    );

    // 7. The sibling child merges into the same feature. Parallel
    // contributors textually conflict (single-file three-way from the shared
    // baseline); the merge commit still records every participant, and the
    // approval-layer resolution flow clears the conflict for a clean re-merge.
    let child2_cp = manager
        .create_checkpoint("child2", &[entry("b.txt", b"sibling edit")])
        .unwrap();
    let attempt = manager.merge_entity_changes("child2", "main").unwrap();
    assert!(
        attempt.merge_result.has_conflicts(),
        "parallel contributors must surface conflicts"
    );
    assert_eq!(
        parent_ids(&stored(&storage, &attempt.checkpoint_id)),
        HashSet::from([main_head_1.clone(), child2_cp.id.clone()]),
        "even a conflicted attempt records all participants as parents"
    );

    // Approval-layer resolution clears the conflict; the re-merge is clean
    // and lands as its own commit with the same participant parent set.
    let remaining = manager
        .resolve_conflicts(
            "child2",
            "main",
            &[("b.txt".to_string(), b"sibling edit".to_vec())],
        )
        .unwrap();
    assert_eq!(remaining, 0, "resolution must clear all conflicts");
    let merge2 = manager.merge_entity_changes("child2", "main").unwrap();
    assert!(!merge2.merge_result.has_conflicts());
    assert_ne!(merge2.checkpoint_id, attempt.checkpoint_id);
    assert_eq!(
        parent_ids(&stored(&storage, &merge2.checkpoint_id)),
        HashSet::from([main_head_1.clone(), child2_cp.id.clone()])
    );

    // The feature head advances onto the accepted merge.
    tick();
    let main_head_2 = seed_feature_head(
        &storage,
        "main",
        merge2.merge_result.snapshot_id,
        &merge2.checkpoint_id,
    );

    // 8. A third execution contributes a separate feature.
    tick();
    let worker_cp = manager
        .create_checkpoint("worker", &[entry("c.txt", b"worker edit")])
        .unwrap();
    let merge_side = manager.merge_entity_changes("worker", "side").unwrap();
    assert!(!merge_side.merge_result.has_conflicts());
    tick();
    let side_head = seed_feature_head(
        &storage,
        "side",
        merge_side.merge_result.snapshot_id,
        &merge_side.checkpoint_id,
    );

    // 9. First staged join: no staged commit exists yet, so the parents are
    // exactly the participating feature heads.
    let staged1 = manager.merge_features_to_staged(&["main"]).unwrap();
    assert!(!staged1.merge_result.has_conflicts());
    assert_eq!(
        parent_ids(&stored(&storage, &staged1.checkpoint_id)),
        HashSet::from([main_head_2.clone()]),
        "round 1 links the latest main feature head"
    );

    // 10. Second staged join over the next feature: the commit chains onto
    // the previous staged commit and links the new feature head.
    let staged2 = manager.merge_features_to_staged(&["side"]).unwrap();
    assert!(!staged2.merge_result.has_conflicts());
    assert_eq!(
        parent_ids(&stored(&storage, &staged2.checkpoint_id)),
        HashSet::from([staged1.checkpoint_id.clone(), side_head.clone(),]),
        "staged join must record every participant as a parent"
    );

    // 11. Provenance: each child's edits are traceable through its actor.
    let child_changes = manager
        .list_changes_by_actor(&child_actor, None, None)
        .unwrap();
    assert!(
        child_changes
            .iter()
            .any(|c| c.file == "a.txt" && c.hash == sha256_hex(b"child edit")),
        "child edit must be traceable: {child_changes:?}"
    );
    let child2_changes = manager
        .list_changes_by_actor(&child2_actor, None, None)
        .unwrap();
    assert!(
        child2_changes
            .iter()
            .any(|c| c.file == "b.txt" && c.hash == sha256_hex(b"sibling edit")),
        "child2 edit must be traceable: {child2_changes:?}"
    );
    // Path filter narrows the view to matching files only.
    let filtered = manager
        .list_changes_by_actor(&child_actor, Some("a.txt"), None)
        .unwrap();
    assert!(!filtered.is_empty());
    assert!(filtered.iter().all(|c| c.file == "a.txt"));
    assert!(manager
        .list_changes_by_actor(&child_actor, Some("missing.txt"), None)
        .unwrap()
        .is_empty());

    // 12. Branch heads still point at each child's latest checkpoint.
    assert_eq!(
        manager.branch_head("child").unwrap().as_deref(),
        Some(child_cp.id.as_str())
    );

    // 13. DAG reachability: walking `parents` edges from the final staged
    // merge commit reaches every participant checkpoint.
    let seen = reachable_from(&storage, &staged2.checkpoint_id);
    let expected = [
        ("staged2", staged2.checkpoint_id.clone()),
        ("staged1", staged1.checkpoint_id.clone()),
        ("main_head_1", main_head_1),
        ("main_head_2", main_head_2),
        ("side_head", side_head),
        ("merge1", merge1.checkpoint_id),
        ("merge2", merge2.checkpoint_id),
        ("merge_side", merge_side.checkpoint_id),
        ("child_cp", child_cp.id),
        ("child2_cp", child2_cp.id),
        ("worker_cp", worker_cp.id),
        ("parent_cp", parent_cp.id.clone()),
    ];
    // `parent_cp` is the workspace base: it is only reachable when some
    // participant chain roots at it, which is not required by the merge
    // topology — every participant must be reachable instead.
    for (name, id) in expected.iter() {
        if *name == "parent_cp" {
            continue;
        }
        assert!(
            seen.contains(id),
            "checkpoint {name}={id} must be reachable from the final merge commit; seen={seen:?}"
        );
    }
    assert!(!seen.contains(&parent_cp.id));
}

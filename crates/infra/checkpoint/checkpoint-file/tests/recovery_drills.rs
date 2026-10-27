//! Recovery drills: the six canonical restore paths end to end.
//!
//! - full-commit restore through the unified entry;
//! - scoped restore leaves out-of-range files alone;
//! - selection preview agrees with selection execution;
//! - atomic multi-feature merge pre-validates without touching main;
//! - revert by stable feature name removes only that feature;
//! - strict state-link policy refuses a silent state-only restore.

use std::collections::HashMap;

use checkpoint_file::file::merge::FeatureMergeOptions;
use checkpoint_file::file::{
    FileCheckpointManager, FileCheckpointOptions, FileContentEntry, RestoreTarget,
    StateFileRestorePolicy,
};

fn entry(path: &str, content: &[u8]) -> FileContentEntry {
    FileContentEntry::new(path, content.to_vec())
}

fn manager() -> FileCheckpointManager {
    FileCheckpointManager::new_in_memory().unwrap()
}

fn opts() -> FileCheckpointOptions {
    FileCheckpointOptions::default()
}

#[test]
fn drill_full_commit_restore_roundtrip() {
    let manager = manager();
    let cp = manager
        .create_checkpoint(
            "exec-1",
            &[entry("a.txt", b"v1\n"), entry("sub/b.txt", b"w1\n")],
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("a.txt"), b"dirty\n").unwrap();
    std::fs::write(dir.path().join("sub/b.txt"), b"dirty\n").unwrap();

    let outcome = manager
        .restore_files(
            &RestoreTarget::Commit {
                commit_id: cp.id.clone(),
                prefixes: None,
            },
            dir.path(),
            &opts(),
        )
        .unwrap();
    assert!(outcome.restored >= 2);
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"v1\n");
    assert_eq!(
        std::fs::read(dir.path().join("sub/b.txt")).unwrap(),
        b"w1\n"
    );
}

#[test]
fn drill_scoped_restore_preserves_outside_range() {
    let manager = manager();
    let cp = manager
        .create_checkpoint(
            "exec-1",
            &[entry("a.txt", b"v1\n"), entry("sub/b.txt", b"w1\n")],
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("a.txt"), b"v1\n").unwrap();
    std::fs::write(dir.path().join("sub/b.txt"), b"v1\n").unwrap();
    std::fs::write(dir.path().join("a.txt"), b"dirty\n").unwrap();
    std::fs::write(dir.path().join("sub/b.txt"), b"dirty\n").unwrap();

    let scope = dir.path().join("sub").to_string_lossy().to_string();
    manager
        .restore_files(
            &RestoreTarget::Commit {
                commit_id: cp.id.clone(),
                prefixes: Some(vec![scope]),
            },
            dir.path(),
            &opts(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("sub/b.txt")).unwrap(),
        b"w1\n"
    );
    assert_eq!(
        std::fs::read(dir.path().join("a.txt")).unwrap(),
        b"dirty\n",
        "outside range preserved"
    );
}

#[test]
fn drill_selection_preview_matches_execution() {
    let manager = manager();
    let first = manager
        .create_checkpoint(
            "exec-1",
            &[entry("a.txt", b"v1\n"), entry("b.txt", b"v1\n")],
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), b"dirty\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), b"dirty\n").unwrap();

    let files = HashMap::from([("a.txt".to_string(), first.id.clone())]);
    let preview = manager
        .preview_restore_selected(&files, dir.path(), &opts())
        .unwrap();
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].path, "a.txt");

    let outcome = manager
        .restore_files(
            &RestoreTarget::Selection {
                files: files.clone(),
            },
            dir.path(),
            &opts(),
        )
        .unwrap();
    assert_eq!(outcome.restored, 1);
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"v1\n");
    assert_eq!(
        std::fs::read(dir.path().join("b.txt")).unwrap(),
        b"dirty\n",
        "unlisted paths untouched"
    );

    let settled = manager
        .preview_restore_selected(&files, dir.path(), &opts())
        .unwrap();
    assert!(settled.is_empty(), "repeating the restore is a no-op");
}

#[test]
fn drill_atomic_merge_prevalidates_without_touching_main() {
    let manager = manager();
    manager
        .create_checkpoint("a", &[entry("shared.txt", b"A\n")])
        .unwrap();
    manager.merge_entity_changes("a", "feat-a").unwrap();
    manager
        .create_checkpoint("b", &[entry("shared.txt", b"B\n")])
        .unwrap();
    manager.merge_entity_changes("b", "feat-b").unwrap();

    // Land the first feature so the second one observably conflicts.
    manager.merge_features_to_main(&["feat-a"]).unwrap();
    let main_before = manager.list_partitions().unwrap();
    let snapshot_before = main_before
        .iter()
        .find(|p| p.kind == "main")
        .expect("main partition")
        .current_snapshot
        .clone();

    let previews = manager.preview_merge_features_to_main(&["feat-b"]).unwrap();
    assert_eq!(previews.len(), 1);
    assert!(previews[0].would_conflict);

    let atomic = FeatureMergeOptions {
        atomic: true,
        confirm_order: true,
    };
    let err = manager
        .merge_features_to_main_with_options(&["feat-b"], &atomic)
        .unwrap_err();
    assert!(
        matches!(
            err,
            checkpoint_base::error::CheckpointError::MergeConflict { .. }
        ),
        "atomic pre-validation aborts, got {err:?}"
    );
    let snapshot_after = manager
        .list_partitions()
        .unwrap()
        .into_iter()
        .find(|p| p.kind == "main")
        .expect("main partition")
        .current_snapshot;
    assert_eq!(snapshot_before, snapshot_after, "main untouched");

    // Order must be explicit for multi-feature merges.
    let loose = FeatureMergeOptions {
        atomic: false,
        confirm_order: false,
    };
    let err = manager
        .merge_features_to_main_with_options(&["feat-a", "feat-b"], &loose)
        .unwrap_err();
    assert!(
        matches!(
            err,
            checkpoint_base::error::CheckpointError::Validation { .. }
        ),
        "order confirmation required, got {err:?}"
    );
}

#[test]
fn drill_conflict_resolve_then_merge_cleanly() {
    use wf_types::config::file_checkpoint::ConflictBehavior;

    let manager = manager();
    // Two independent actor lines editing the same path: no common
    // ancestor, so the review-to-feature merge must conflict.
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"one\nline2\n")])
        .unwrap();
    manager.merge_entity_changes("exec-1", "feature-1").unwrap();

    manager
        .create_checkpoint("exec-2", &[entry("a.txt", b"two\nline2\n")])
        .unwrap();
    let outcome = manager
        .approve_changes("exec-2", "feature-1", None, ConflictBehavior::Marker, None)
        .unwrap();
    assert!(outcome.has_conflicts());

    // The single authoritative gate reports the feature unresolved.
    let status = manager
        .conflict_status_for_ref("refs/wf/feat/feature-1")
        .unwrap();
    assert!(status.unresolved);
    assert!(status.files.contains(&"a.txt".to_string()));

    let remaining = manager
        .resolve_conflicts(
            "exec-2",
            "feature-1",
            &[("a.txt".to_string(), b"resolved\nline2\n".to_vec())],
        )
        .unwrap();
    assert_eq!(remaining, 0);
    assert!(
        !manager
            .conflict_status_for_ref("refs/wf/feat/feature-1")
            .unwrap()
            .unresolved,
        "resolved feature reads clean through the same gate"
    );

    let joined = manager.merge_branch_changes(&["feature-1"]).unwrap();
    assert!(!joined.merge_result.has_conflicts());
    assert_eq!(
        manager
            .read_file_at(&joined.checkpoint_id, "a.txt")
            .unwrap(),
        Some(b"resolved\nline2\n".to_vec())
    );
}

#[test]
fn drill_revert_by_stable_feature_name() {
    let manager = manager();
    manager
        .create_checkpoint("exec-a", &[entry("a.txt", b"A\n")])
        .unwrap();
    manager.merge_entity_changes("exec-a", "feat-a").unwrap();
    manager
        .create_checkpoint("exec-b", &[entry("b.txt", b"B\n")])
        .unwrap();
    manager.merge_entity_changes("exec-b", "feat-b").unwrap();
    let joined = manager.merge_branch_changes(&["feat-a", "feat-b"]).unwrap();

    // Stable revert by feature name: only feat-a content is backed out.
    let reverted = manager
        .revert_feature_by_name("feat-a", "rollback")
        .unwrap();
    assert_ne!(reverted, joined.checkpoint_id);
    assert!(
        manager.read_file_at(&reverted, "a.txt").unwrap().is_none(),
        "reverted feature must be gone"
    );
    assert_eq!(
        manager.read_file_at(&reverted, "b.txt").unwrap(),
        Some(b"B\n".to_vec()),
        "other feature intact"
    );
}

#[test]
fn drill_strict_state_link_policy_rejects_missing() {
    let manager = manager();
    let cp = manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"v1\n")])
        .unwrap();
    manager.record_state_file_link("state-1", &cp.id).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), b"dirty\n").unwrap();

    let linked = manager
        .materialize_state_files_with_policy(
            "state-1",
            dir.path(),
            &opts(),
            StateFileRestorePolicy::Strict,
        )
        .unwrap()
        .expect("linked state materializes");
    assert!(linked.restored >= 1);

    // Lenient keeps the historical None contract for pre-link history.
    assert!(manager
        .materialize_state_files_with_policy(
            "state-unknown",
            dir.path(),
            &opts(),
            StateFileRestorePolicy::Lenient,
        )
        .unwrap()
        .is_none());
    // Strict refuses a silent state-only restore.
    let err = manager
        .materialize_state_files_with_policy(
            "state-unknown",
            dir.path(),
            &opts(),
            StateFileRestorePolicy::Strict,
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            checkpoint_base::error::CheckpointError::Validation { .. }
        ),
        "strict policy must fail loudly, got {err:?}"
    );
}

#[test]
fn drill_watermark_reports_reclaimable_tiers() {
    let manager = manager();
    let actor = manager.actor_id_for("entity-1");
    manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();

    let watermark = manager.store_watermark().unwrap();
    assert!(watermark.reachable_commits >= 1);
    assert!(
        watermark.loose_objects >= watermark.reachable_commits + watermark.reclaimable_commits,
        "tiers partition the loose objects"
    );
    assert_eq!(
        manager.estimate_reclaimable_commits().unwrap(),
        watermark.reclaimable_commits
    );

    let report = manager
        .run_gc_report(checkpoint_file::gc::GcRetention::default())
        .unwrap();
    assert_eq!(
        report.watermark_after.reclaimable_commits, 0,
        "a fresh sweep leaves nothing reclaimable"
    );
}

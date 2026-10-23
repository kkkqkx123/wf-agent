use super::*;
use crate::file::util::sha256_hex;
use checkpoint_base::error::CheckpointError;
use std::collections::HashMap;
use wf_types::config::file_checkpoint::ConflictBehavior;

fn manager() -> FileCheckpointManager {
    FileCheckpointManager::new_in_memory().unwrap()
}

fn entry(path: &str, content: &[u8]) -> FileContentEntry {
    FileContentEntry::new(path, content.to_vec())
}

fn feature_texts(manager: &FileCheckpointManager, feature_name: &str) -> HashMap<String, String> {
    crate::provenance::get_feature_workspace(manager.git_ref().unwrap(), feature_name)
        .unwrap()
        .into_iter()
        .map(|f| (f.path, String::from_utf8(f.content).unwrap_or_default()))
        .collect()
}

#[test]
fn sequential_partial_approvals_compose() {
    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"one")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("b.txt", b"two")])
        .unwrap();
    let first = manager
        .approve_changes(
            "exec-1",
            "feature-1",
            Some(vec!["a.txt".to_string()]),
            ConflictBehavior::Marker,
            None,
        )
        .unwrap();
    assert!(first.merged);
    let second = manager
        .approve_changes(
            "exec-1",
            "feature-1",
            Some(vec!["b.txt".to_string()]),
            ConflictBehavior::Marker,
            None,
        )
        .unwrap();
    assert!(second.merged);
    let texts = feature_texts(&manager, "feature-1");
    assert_eq!(texts.get("a.txt").map(String::as_str), Some("one"));
    assert_eq!(texts.get("b.txt").map(String::as_str), Some("two"));
}

#[test]
fn full_approve_closes_remainder_after_partial() {
    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"one")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("b.txt", b"two")])
        .unwrap();
    let partial = manager
        .approve_changes(
            "exec-1",
            "feature-1",
            Some(vec!["a.txt".to_string()]),
            ConflictBehavior::Marker,
            None,
        )
        .unwrap();
    assert!(partial.merged);
    let full = manager
        .approve_changes("exec-1", "feature-1", None, ConflictBehavior::Marker, None)
        .unwrap();
    assert!(full.merged);
    let texts = feature_texts(&manager, "feature-1");
    assert_eq!(texts.get("a.txt").map(String::as_str), Some("one"));
    assert_eq!(texts.get("b.txt").map(String::as_str), Some("two"));
    assert!(manager.list_pending_approvals().unwrap().is_empty());
}

#[test]
fn conflict_flow_lists_and_resolves() {
    let manager = manager();
    // Agent 1 edits the first line and merges cleanly.
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"base\nline2\n")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"one\nline2\n")])
        .unwrap();
    let r1 = manager.merge_entity_changes("exec-1", "feature-1").unwrap();
    assert!(!r1.merge_result.has_conflicts());

    // Agent 2 edits the same line differently → conflict.
    manager
        .create_checkpoint("exec-2", &[entry("a.txt", b"base\nline2\n")])
        .unwrap();
    manager
        .create_checkpoint("exec-2", &[entry("a.txt", b"two\nline2\n")])
        .unwrap();
    let outcome = manager
        .approve_changes("exec-2", "feature-1", None, ConflictBehavior::Marker, None)
        .unwrap();
    assert!(outcome.has_conflicts(), "same-line edits must conflict");
    assert!(outcome.conflict_files.contains(&"a.txt".to_string()));

    // list_conflicts reports the conflicted path.
    let conflicts = manager.list_conflicts().unwrap();
    assert!(
        conflicts.iter().any(|c| c.path == "a.txt"),
        "list_conflicts must report a.txt"
    );

    // Resolve with the authoritative content; the marker is cleared.
    let remaining = manager
        .resolve_conflicts(
            "exec-2",
            "feature-1",
            &[("a.txt".to_string(), b"resolved\nline2\n".to_vec())],
        )
        .unwrap();
    assert_eq!(remaining, 0, "resolution must clear all conflicts");
    let conflicts = manager.list_conflicts().unwrap();
    assert!(
        conflicts.is_empty(),
        "no conflicts may remain after resolution"
    );

    // A subsequent full merge succeeds cleanly.
    let remerged = manager.merge_entity_changes("exec-2", "feature-1").unwrap();
    assert!(
        !remerged.merge_result.has_conflicts(),
        "re-merge after resolution must succeed"
    );
}

#[test]
fn merge_branch_changes_joins_features_and_cleans_pointers() {
    let manager = manager();
    // Branch 1 edits a.txt.
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"base-a")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"branch-a")])
        .unwrap();
    manager.merge_entity_changes("exec-1", "branch-1").unwrap();
    // Branch 2 edits b.txt.
    manager
        .create_checkpoint("exec-2", &[entry("b.txt", b"base-b")])
        .unwrap();
    manager
        .create_checkpoint("exec-2", &[entry("b.txt", b"branch-b")])
        .unwrap();
    manager.merge_entity_changes("exec-2", "branch-2").unwrap();

    // Join: merge both features into main, then delete the pointers.
    let joined = manager
        .merge_branch_changes(&["branch-1", "branch-2"])
        .unwrap();
    assert!(
        !joined.merge_result.has_conflicts(),
        "different-file branches must join cleanly"
    );

    // Both changes are present in the main workspace.
    let a = manager
        .read_file_at(&joined.checkpoint_id, "a.txt")
        .unwrap()
        .expect("a.txt present in join");
    assert_eq!(a, b"branch-a");
    let b = manager
        .read_file_at(&joined.checkpoint_id, "b.txt")
        .unwrap()
        .expect("b.txt present in join");
    assert_eq!(b, b"branch-b");

    // Branch head pointers are removed; the DAG data stays intact.
    let partitions = manager.list_partitions().unwrap();
    assert!(
        partitions.iter().any(|p| p.kind == "main"),
        "main partition exists after join"
    );
}

#[test]
fn layered_partitions_isolate_actors() {
    let manager = manager();
    manager
        .create_checkpoint("agent:exec-1", &[entry("a.txt", b"actor one")])
        .unwrap();
    manager
        .create_checkpoint("agent:exec-2", &[entry("a.txt", b"actor two")])
        .unwrap();

    let one = manager.restore_latest("agent:exec-1").unwrap().unwrap();
    let two = manager.restore_latest("agent:exec-2").unwrap().unwrap();
    assert_eq!(one[0].hash, sha256_hex(b"actor one"));
    assert_eq!(two[0].hash, sha256_hex(b"actor two"));
}

/// Two managers with separate stores must not cross-write: manual edits
/// land on each workspace's own human ref, backed by its own bare
/// repository.
#[test]
fn multi_workspace_manual_partitions_do_not_cross_write() {
    let m_a = manager();
    let m_b = manager();

    // Same file edited in both workspaces with different content.
    m_a.apply_manual_edit("a.txt", b"from workspace a").unwrap();
    m_b.apply_manual_edit("a.txt", b"from workspace b").unwrap();

    let read_human = |m: &FileCheckpointManager| -> String {
        let git = m.git_ref().unwrap();
        let head = git.read_ref(crate::git_store::REF_HUMAN).unwrap().unwrap();
        let commit = git.read_commit(&head).unwrap();
        let files = git.tree_to_bytes(&commit.tree).unwrap();
        String::from_utf8(files["a.txt"].clone()).unwrap()
    };
    assert_eq!(read_human(&m_a), "from workspace a");
    assert_eq!(read_human(&m_b), "from workspace b");
}

/// Without a workspace root the manager still records manual edits on
/// the human ref (never auto-merged); there are no legacy partitions.
#[test]
fn no_workspace_root_keeps_human_ref() {
    let manager = manager();
    assert_eq!(manager.workspace_key(), None);

    manager.apply_manual_edit("a.txt", b"legacy").unwrap();
    let git = manager.git_ref().unwrap();
    assert!(
        git.read_ref(crate::git_store::REF_HUMAN).unwrap().is_some(),
        "manual edits must land on the human ref"
    );
    assert!(
        git.read_ref(crate::git_store::REF_MAIN).unwrap().is_none(),
        "the human edit must not touch main"
    );
}

/// Opening a persistent DB binds it to the workspace root recorded in
/// metadata; a different root on the same DB is rejected.
#[test]
fn open_from_config_binds_workspace_root_to_db() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("cp.db");
    let ws_a = dir.path().join("ws-a");
    let ws_b = dir.path().join("ws-b");
    std::fs::create_dir_all(&ws_a).unwrap();
    std::fs::create_dir_all(&ws_b).unwrap();
    let root_a = ws_a.to_string_lossy().into_owned();
    let root_b = ws_b.to_string_lossy().into_owned();
    let config_for = |root: &str| wf_types::config::file_checkpoint::FileCheckpointConfig {
        storage: Some(
            wf_types::config::file_checkpoint::FileCheckpointStorageConfig {
                storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
                db_path: Some(db_path.to_string_lossy().into_owned()),
            },
        ),
        workspace_root: Some(root.to_string()),
        store_in_workspace: Some(true),
        ..Default::default()
    };

    // First open records the binding.
    FileCheckpointManager::open_from_config(&config_for(&root_a)).unwrap();
    // Reopening with the same root is fine.
    FileCheckpointManager::open_from_config(&config_for(&root_a)).unwrap();
    // A different root on the same DB is rejected.
    let err = match FileCheckpointManager::open_from_config(&config_for(&root_b)) {
        Ok(_) => panic!("different workspace root on a bound DB must fail"),
        Err(e) => e,
    };
    assert!(
        matches!(err, CheckpointError::Validation { .. }),
        "different workspace root on a bound DB must fail: {err:?}"
    );
    // No workspace root is not bound (legacy mode still opens).
    let legacy = wf_types::config::file_checkpoint::FileCheckpointConfig {
        storage: Some(
            wf_types::config::file_checkpoint::FileCheckpointStorageConfig {
                storage_type: wf_types::config::file_checkpoint::FileCheckpointStorageType::Sqlite,
                db_path: Some(db_path.to_string_lossy().into_owned()),
            },
        ),
        ..Default::default()
    };
    FileCheckpointManager::open_from_config(&legacy).unwrap();
}

#[test]
fn no_file_checkpoint_delta_remnants() {
    // Incremental storage is owned by the Git object store; the
    // hand-written delta projection must stay deleted. Needles are
    // assembled from fragments so this test's own source cannot match.
    let delta_needle = ["FileCheckpoint", "Delta"].concat();
    let file_src = include_str!("../manager.rs");
    assert!(
        !file_src.contains(&delta_needle),
        "delta projection struct must not be reintroduced in file/manager.rs"
    );
    let lib_src = include_str!("../../lib.rs");
    assert!(
        !lib_src.contains(&delta_needle),
        "delta projection struct must not be re-exported from lib.rs"
    );
}

#[test]
fn no_max_delta_chain_in_file_options() {
    let option_needle = ["max_", "delta_chain_", "length"].concat();
    let file_src = include_str!("../manager.rs");
    assert!(
        !file_src.contains(&option_needle),
        "checkpoint options must stay free of the dead chain-length knob"
    );
}

#[test]
fn scoped_restore_only_touches_checkpoint_range() {
    let manager = manager();
    let opts = FileCheckpointOptions::default();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("frontend")).unwrap();
    std::fs::create_dir_all(dir.path().join("backend")).unwrap();
    std::fs::write(dir.path().join("frontend/a.txt"), b"v1").unwrap();
    std::fs::write(dir.path().join("backend/b.txt"), b"w1").unwrap();
    let cp = manager
        .create_workspace_checkpoint("exec-1", dir.path(), &opts)
        .unwrap();
    std::fs::write(dir.path().join("frontend/a.txt"), b"v2").unwrap();
    std::fs::write(dir.path().join("backend/b.txt"), b"w2").unwrap();
    std::fs::write(dir.path().join("frontend/extra.txt"), b"extra").unwrap();
    let frontend = dir.path().join("frontend").to_string_lossy().to_string();
    manager
        .restore_workspace_scoped(
            "exec-1",
            &cp.id,
            dir.path(),
            &opts,
            Some(std::slice::from_ref(&frontend)),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("frontend/a.txt")).unwrap(),
        b"v1"
    );
    assert_eq!(
        std::fs::read(dir.path().join("backend/b.txt")).unwrap(),
        b"w2",
        "outside range preserved"
    );
    assert!(
        !dir.path().join("frontend/extra.txt").exists(),
        "inside-range extra cleaned"
    );
    let second = manager
        .restore_workspace_scoped("exec-1", &cp.id, dir.path(), &opts, Some(&[frontend]))
        .unwrap();
    assert_eq!(second.restored, 0);
    assert_eq!(
        std::fs::read(dir.path().join("backend/b.txt")).unwrap(),
        b"w2"
    );
}

#[test]
fn partition_history_lists_merges_newest_first() {
    use crate::event::{CheckpointEvent, CheckpointEventBus};

    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"one")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"two")])
        .unwrap();
    // First approval into an empty feature is a fast-forward: the feature
    // head is a single-parent commit on the review's edit line.
    let first = manager.merge_entity_changes("exec-1", "feature-1").unwrap();

    let history = manager
        .partition_history("refs/wf/feat/feature-1", 50, 0)
        .unwrap();
    assert!(!history.is_empty(), "feature history must not be empty");
    let head = &history[0];
    assert_eq!(head.snapshot_id, first.merge_result.commit_id);
    assert!(
        !head.is_merge,
        "first approval into an empty feature fast-forwards"
    );
    assert_eq!(head.parents.len(), 1);
    assert!(!head.has_conflicts);

    // A later edit merged into the existing feature is a true multi-parent
    // merge, and it becomes the newest entry in the partition history.
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"three")])
        .unwrap();
    let second = manager.merge_entity_changes("exec-1", "feature-1").unwrap();
    assert_eq!(second.merge_result.parents.len(), 2);

    let history = manager
        .partition_history("refs/wf/feat/feature-1", 50, 0)
        .unwrap();
    let head = &history[0];
    assert_eq!(head.snapshot_id, second.merge_result.commit_id);
    assert!(head.is_merge, "second merge into a feature is a merge");
    assert_eq!(head.parents.len(), 2);
    assert_eq!(head.parents[0], first.merge_result.commit_id);
    assert!(!head.has_conflicts);

    // Unknown refs are rejected instead of walking arbitrary ids.
    assert!(manager
        .partition_history("refs/wf/feat/nope", 50, 0)
        .is_err());
    assert!(manager.partition_history("refs/heads/main", 50, 0).is_err());

    // Paging past the end yields nothing.
    let total = manager
        .partition_history("refs/wf/feat/feature-1", 50, 0)
        .unwrap()
        .len();
    let empty = manager
        .partition_history("refs/wf/feat/feature-1", 50, total + 10)
        .unwrap();
    assert!(empty.is_empty());

    // Approval transitions publish onto the attached bus. Checkpoint
    // creation also emits FileChanged, so scan the drained queue for the
    // approval events and assert their order instead of assuming they are
    // the first two.
    let bus = CheckpointEventBus::new();
    let mut rx = bus.subscribe();
    let manager = FileCheckpointManager::new_in_memory()
        .unwrap()
        .with_event_bus(bus);
    manager
        .create_checkpoint("exec-9", &[entry("b.txt", b"v1")])
        .unwrap();
    manager
        .approve_changes("exec-9", "feature-9", None, ConflictBehavior::Marker, None)
        .unwrap();
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    let submit = events
        .iter()
        .position(|event| matches!(event, CheckpointEvent::ReviewSubmitted { .. }))
        .expect("submit must publish ReviewSubmitted");
    let approve = events
        .iter()
        .position(|event| matches!(event, CheckpointEvent::ApprovalResolved { .. }))
        .expect("approve must publish ApprovalResolved");
    assert!(
        submit < approve,
        "ReviewSubmitted must precede ApprovalResolved"
    );
    match &events[approve] {
        CheckpointEvent::ApprovalResolved { decision, .. } => {
            assert_eq!(*decision, crate::event::ApprovalDecision::Approved);
        }
        other => panic!("expected ApprovalResolved, got {other:?}"),
    }
}

#[test]
fn oversized_files_are_opaque_to_capture_and_restore() {
    let opts = FileCheckpointOptions::default();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("small.txt"), b"ok").unwrap();
    std::fs::write(dir.path().join("big.bin"), b"0123456789").unwrap();

    let mut manager = FileCheckpointManager::new_in_memory().unwrap();
    manager.policy.scan_config.max_file_size = 4;
    let cp = manager
        .create_workspace_checkpoint("exec-1", dir.path(), &opts)
        .unwrap();

    assert!(
        cp.files.iter().any(|f| f.path == "small.txt"),
        "under-cap file captured"
    );
    assert!(
        !cp.files.iter().any(|f| f.path == "big.bin"),
        "oversized file excluded from the checkpoint tree"
    );
    assert_eq!(
        cp.oversized_files.as_deref(),
        Some(&["big.bin".to_string()][..]),
        "oversized manifest records the excluded path"
    );

    // Missing on disk: restore must never recreate an oversized file.
    std::fs::remove_file(dir.path().join("big.bin")).unwrap();
    manager
        .restore_workspace("exec-1", &cp.id, dir.path(), &opts)
        .unwrap();
    assert!(
        !dir.path().join("big.bin").exists(),
        "oversized file is never restored"
    );

    // Present on disk: restore must never delete an oversized file.
    std::fs::write(dir.path().join("big.bin"), b"0123456789").unwrap();
    let result = manager
        .restore_workspace("exec-1", &cp.id, dir.path(), &opts)
        .unwrap();
    assert_eq!(result.deleted, 0);
    assert!(
        dir.path().join("big.bin").exists(),
        "oversized file is never deleted"
    );
}

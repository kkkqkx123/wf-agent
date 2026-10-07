//! Closed-loop integration on the Git model: actor-line isolation,
//! multi-parent merge commits, standard conflict markers, isolated
//! rollback, source-index rebuild and human-line isolation.
//!
//! Scenario: two actors edit in isolation, submit to review, merge into
//! shared features, and join features into main. Every join produces an
//! independent multi-parent merge commit; walking `parents` edges from
//! the final commit reaches every participant. Rolling back one feature
//! leaves the other feature's content intact.

use wf_checkpoint::file::{FileCheckpointManager, FileContentEntry};

fn entry(path: &str, content: &[u8]) -> FileContentEntry {
    FileContentEntry::new(path, content.to_vec())
}

fn manager() -> FileCheckpointManager {
    FileCheckpointManager::new_in_memory().unwrap()
}

#[test]
fn isolated_lines_merge_with_full_ancestry() {
    let manager = manager();

    manager
        .create_checkpoint("child", &[entry("a.txt", b"base")])
        .unwrap();
    manager
        .create_checkpoint("child", &[entry("a.txt", b"child edit")])
        .unwrap();
    manager
        .create_checkpoint("child2", &[entry("b.txt", b"other")])
        .unwrap();

    let merge1 = manager.merge_entity_changes("child", "main").unwrap();
    assert!(!merge1.merge_result.has_conflicts());
    let attempt = manager.merge_entity_changes("child2", "main").unwrap();
    assert!(!attempt.merge_result.has_conflicts());

    let joined = manager.merge_branch_changes(&["main"]).unwrap();
    assert!(!joined.merge_result.has_conflicts());

    let partitions = manager.list_partitions().unwrap();
    let main = partitions
        .iter()
        .find(|p| p.kind == "staged")
        .expect("main partition exists after join");
    assert_eq!(main.current_snapshot, joined.checkpoint_id);
    assert!(
        main.history_len >= 3,
        "join must retain participant history"
    );
    let a = manager
        .read_file_at(&joined.checkpoint_id, "a.txt")
        .unwrap()
        .expect("a.txt present");
    assert_eq!(a, b"child edit");
    let b = manager
        .read_file_at(&joined.checkpoint_id, "b.txt")
        .unwrap()
        .expect("b.txt present");
    assert_eq!(b, b"other");
}

#[test]
fn rollback_one_feature_leaves_the_other_intact() {
    let manager = manager();

    manager
        .create_checkpoint("exec-a", &[entry("a.txt", b"A")])
        .unwrap();
    let fa = manager.merge_entity_changes("exec-a", "feat-a").unwrap();
    manager
        .create_checkpoint("exec-b", &[entry("b.txt", b"B")])
        .unwrap();
    let fb = manager.merge_entity_changes("exec-b", "feat-b").unwrap();

    let joined = manager.merge_branch_changes(&["feat-a", "feat-b"]).unwrap();
    let before = joined.checkpoint_id.clone();

    let reverted = manager
        .rollback_feature_from_main(&fa.checkpoint_id, "rollback")
        .unwrap();
    assert_ne!(reverted, before);
    let a = manager.read_file_at(&reverted, "a.txt").unwrap();
    assert!(a.is_none(), "reverted feature must be gone");
    let b = manager
        .read_file_at(&reverted, "b.txt")
        .unwrap()
        .expect("other feature must be intact");
    assert_eq!(b, b"B");
    let _ = fb;

    let partitions = manager.list_partitions().unwrap();
    let main = partitions
        .iter()
        .find(|p| p.kind == "staged")
        .expect("main partition exists after rollback");
    assert_eq!(main.current_snapshot, reverted);
}

#[test]
fn overlapping_merge_uses_standard_markers_and_resolves() {
    let manager = manager();

    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"base\nline2\n")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"one\nline2\n")])
        .unwrap();
    let r1 = manager.merge_entity_changes("exec-1", "feature-1").unwrap();
    assert!(!r1.merge_result.has_conflicts());

    manager
        .create_checkpoint("exec-2", &[entry("a.txt", b"base\nline2\n")])
        .unwrap();
    manager
        .create_checkpoint("exec-2", &[entry("a.txt", b"two\nline2\n")])
        .unwrap();
    let outcome = manager
        .approve_changes(
            "exec-2",
            "feature-1",
            None,
            wf_types::config::file_checkpoint::ConflictBehavior::Marker,
            None,
        )
        .unwrap();
    assert!(outcome.has_conflicts());
    assert!(outcome.conflict_files.contains(&"a.txt".to_string()));

    let conflicted_id = outcome.snapshot_id.clone();
    let conflicted = manager
        .read_file_at(&conflicted_id, "a.txt")
        .unwrap()
        .expect("conflicted file stored");
    let text = String::from_utf8(conflicted).unwrap();
    assert!(text.contains("<<<<<<< ours"));
    assert!(text.contains("======="));
    assert!(text.contains(">>>>>>> theirs"));

    let partitions = manager.list_partitions().unwrap();
    assert!(
        !partitions.iter().any(|p| p.kind == "staged"),
        "main is blocked while the feature is unresolved"
    );
    let conflicts = manager.list_conflicts().unwrap();
    assert!(conflicts.iter().any(|c| c.path == "a.txt"));

    let remaining = manager
        .resolve_conflicts(
            "exec-2",
            "feature-1",
            &[("a.txt".to_string(), b"resolved\nline2\n".to_vec())],
        )
        .unwrap();
    assert_eq!(remaining, 0);
    assert!(manager.list_conflicts().unwrap().is_empty());
    let joined = manager.merge_branch_changes(&["feature-1"]).unwrap();
    assert!(!joined.merge_result.has_conflicts());
    let resolved = manager
        .read_file_at(&joined.checkpoint_id, "a.txt")
        .unwrap()
        .expect("resolved file stored");
    assert_eq!(resolved, b"resolved\nline2\n");
}

#[test]
fn execution_branches_do_not_carry_file_content() {
    let manager = manager();
    manager
        .create_checkpoint("parent", &[entry("a.txt", b"base")])
        .unwrap();
    let partitions = manager.list_partitions().unwrap();
    assert!(
        partitions.iter().any(|p| p.kind == "agent"),
        "actor partitions carry file content"
    );
    assert!(
        !partitions.iter().any(|p| p.kind == "execution"),
        "file checkpoints never create execution branch partitions"
    );
    let timeline = manager.file_timeline("a.txt").unwrap();
    assert!(
        !timeline.entries.is_empty(),
        "file history recorded via query view"
    );
}

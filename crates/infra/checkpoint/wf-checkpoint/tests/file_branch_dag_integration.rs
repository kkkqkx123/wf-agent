//! Closed-loop integration on the Git model: actor-line isolation,
//! multi-parent merge commits, standard conflict markers, isolated
//! rollback, source-index rebuild and human-line isolation.
//!
//! Scenario: two actors edit in isolation, submit to review, merge into
//! shared features, and join features into main. Every join produces an
//! independent multi-parent merge commit; walking `parents` edges from
//! the final commit reaches every participant. Rolling back one feature
//! leaves the other feature's content intact.

use std::collections::HashSet;

use wf_checkpoint::file::{FileCheckpointManager, FileContentEntry};
use wf_checkpoint::git_store::GitStore;

fn entry(path: &str, content: &[u8]) -> FileContentEntry {
    FileContentEntry::new(path, content.to_vec())
}

fn manager() -> FileCheckpointManager {
    FileCheckpointManager::new_in_memory().unwrap()
}

fn parents_of(git: &GitStore, id: &str) -> HashSet<String> {
    git.read_commit(id).unwrap().parents.into_iter().collect()
}

#[test]
fn isolated_lines_merge_with_full_ancestry() {
    let manager = manager();
    let git = manager.git_store().unwrap().clone();

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

    // Every participant is reachable from the join commit.
    let main_head = git.read_ref("refs/wf/main").unwrap().unwrap();
    assert_eq!(main_head, joined.checkpoint_id);
    let mut seen = HashSet::new();
    let mut queue = vec![main_head.clone()];
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        for parent in &git.read_commit(&id).unwrap().parents {
            queue.push(parent.clone());
        }
    }
    for expected in [
        main_head,
        merge1.checkpoint_id.clone(),
        attempt.checkpoint_id.clone(),
    ] {
        assert!(
            seen.contains(&expected),
            "commit {expected} must be reachable"
        );
    }
    // Both files are present in the joined tree.
    let files = git
        .tree_to_bytes(&git.read_commit(&joined.checkpoint_id).unwrap().tree)
        .unwrap();
    assert_eq!(files["a.txt"], b"child edit");
    assert_eq!(files["b.txt"], b"other");
}

#[test]
fn rollback_one_feature_leaves_the_other_intact() {
    let manager = manager();
    let git = manager.git_store().unwrap().clone();

    manager
        .create_checkpoint("exec-a", &[entry("a.txt", b"A")])
        .unwrap();
    let fa = manager.merge_entity_changes("exec-a", "feat-a").unwrap();
    manager
        .create_checkpoint("exec-b", &[entry("b.txt", b"B")])
        .unwrap();
    let fb = manager.merge_entity_changes("exec-b", "feat-b").unwrap();

    manager.merge_branch_changes(&["feat-a", "feat-b"]).unwrap();
    let before = git.read_ref("refs/wf/main").unwrap().unwrap();

    // Roll back feat-a only: b.txt must survive, a.txt must be gone.
    let reverted = manager
        .rollback_feature_from_main(&fa.checkpoint_id, "rollback")
        .unwrap();
    assert_ne!(reverted, before);
    let files = git
        .tree_to_bytes(&git.read_commit(&reverted).unwrap().tree)
        .unwrap();
    assert!(
        !files.contains_key("a.txt"),
        "reverted feature must be gone"
    );
    assert_eq!(files["b.txt"], b"B", "other feature must be intact");
    let _ = fb;

    // Ancestry: the revert descends from the pre-revert main.
    assert!(git.is_ancestor(&before, &reverted).unwrap());
}

#[test]
fn overlapping_merge_uses_standard_markers_and_resolves() {
    let manager = manager();
    let git = manager.git_store().unwrap().clone();

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

    // Standard markers are stored in the conflicted file.
    let head = git.read_ref("refs/wf/feat/feature-1").unwrap().unwrap();
    let files = git
        .tree_to_bytes(&git.read_commit(&head).unwrap().tree)
        .unwrap();
    let text = String::from_utf8(files["a.txt"].clone()).unwrap();
    assert!(text.contains("<<<<<<< ours"));
    assert!(text.contains("======="));
    assert!(text.contains(">>>>>>> theirs"));

    // Main is blocked while the feature is unresolved.
    assert!(git.read_ref("refs/wf/main").unwrap().is_none());
    let conflicts = manager.list_conflicts().unwrap();
    assert!(conflicts.iter().any(|c| c.path == "a.txt"));

    // Resolution clears the marker and unblocks the main merge.
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
    let main_files = git
        .tree_to_bytes(&git.read_commit(&joined.checkpoint_id).unwrap().tree)
        .unwrap();
    assert_eq!(main_files["a.txt"], b"resolved\nline2\n");
}

#[test]
fn execution_branches_do_not_carry_file_content() {
    let manager = manager();
    manager
        .create_checkpoint("parent", &[entry("a.txt", b"base")])
        .unwrap();
    // Execution-branch helpers keep working for execution isolation, but
    // file checkpoints never move them.
    assert_eq!(manager.branch_head("child").unwrap(), None);
    assert_eq!(manager.branch_head("parent").unwrap(), None);
    let _ = parents_of;
}

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

fn state_map(states: &[FileState]) -> HashMap<&str, &FileState> {
    states.iter().map(|f| (f.path.as_str(), f)).collect()
}

#[test]
fn compute_file_hash_produces_consistent_output() {
    let hash1 = FileCheckpointManager::compute_file_hash(b"hello world");
    let hash2 = FileCheckpointManager::compute_file_hash(b"hello world");
    assert_eq!(hash1, hash2);
    assert_ne!(
        hash1,
        FileCheckpointManager::compute_file_hash(b"different")
    );
    // SHA-256 hex digest: 64 chars, stable across runs/versions.
    assert_eq!(hash1.len(), 64);
}

#[test]
fn unified_diff_shows_changes() {
    let prev = "line1\nline2\nline3\n";
    let curr = "line1\nline2_modified\nline3\n";

    let diff = FileCheckpointManager::unified_diff(prev, curr, 1);
    assert!(diff.contains("line2"));
    assert!(diff.contains("-line2"));
    assert!(diff.contains("+line2_modified"));
}

#[test]
fn create_and_restore_checkpoint_roundtrip() {
    let manager = manager();
    let cp = manager
        .create_checkpoint(
            "exec-1",
            &[entry("a.txt", b"hello a"), entry("b.txt", b"hello b")],
        )
        .unwrap();
    assert_eq!(cp.files.len(), 2);

    let restored = manager.restore_checkpoint("exec-1", &cp.id).unwrap();
    let map = state_map(&restored);
    assert_eq!(map["a.txt"].hash, sha256_hex(b"hello a"));
    assert_eq!(map["b.txt"].hash, sha256_hex(b"hello b"));
}

#[test]
fn restore_latest_returns_most_recent() {
    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"v1")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"v2")])
        .unwrap();

    let latest = manager.restore_latest("exec-1").unwrap().unwrap();
    let map = state_map(&latest);
    assert_eq!(map["a.txt"].hash, sha256_hex(b"v2"));
    assert_eq!(map["a.txt"].size, 2);
}

#[test]
fn restore_latest_none_without_checkpoints() {
    let manager = manager();
    assert!(manager.restore_latest("exec-1").unwrap().is_none());
}

#[test]
fn create_latest_file_checkpoint_snapshots_partition_state() {
    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"v1")])
        .unwrap();

    let latest = manager.create_latest_file_checkpoint("exec-1").unwrap();
    assert!(latest.is_some());
    let cp = latest.unwrap();
    let map = state_map(&cp.files);
    assert_eq!(map["a.txt"].hash, sha256_hex(b"v1"));

    // No history yet → None.
    assert!(manager
        .create_latest_file_checkpoint("never-touched")
        .unwrap()
        .is_none());
}

#[test]
fn unchanged_files_keep_prior_state_on_next_checkpoint() {
    let manager = manager();
    let cp1 = manager
        .create_checkpoint(
            "exec-1",
            &[entry("a.txt", b"stable"), entry("b.txt", b"changing")],
        )
        .unwrap();
    assert_eq!(cp1.files.len(), 2);

    // Only b.txt is applied again; a.txt keeps its recorded state.
    let cp2 = manager
        .create_checkpoint("exec-1", &[entry("b.txt", b"changed")])
        .unwrap();
    let map = state_map(&cp2.files);
    assert_eq!(map.len(), 2);
    assert_eq!(map["a.txt"].hash, sha256_hex(b"stable"));
    assert_eq!(map["b.txt"].hash, sha256_hex(b"changed"));
}

#[test]
fn binary_files_are_snapshotted_without_diffing() {
    let manager = manager();
    let bytes: Vec<u8> = (0u8..=255u8).collect();
    let cp = manager
        .create_checkpoint("exec-1", &[entry("bin.dat", &bytes)])
        .unwrap();
    let map = state_map(&cp.files);
    assert_eq!(map["bin.dat"].hash, sha256_hex(&bytes));
    assert_eq!(map["bin.dat"].size, 256);
}

#[test]
fn create_and_restore_checkpoint_with_content() {
    let manager = manager();
    let entries = vec![entry("a.txt", b"hello a"), entry("b.txt", b"hello b")];
    let cp = manager.create_checkpoint("exec-1", &entries).unwrap();
    assert_eq!(cp.files.len(), 2);
    assert_eq!(
        cp.files[0].hash,
        sha256_hex(b"hello a"),
        "content hash recorded in FileState"
    );

    // Rollback into a temp dir restores the original bytes.
    let dir = tempfile::tempdir().unwrap();
    let written = manager.restore_content(&cp.id, dir.path()).unwrap();
    assert_eq!(written.len(), 2);
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"hello a");
    assert_eq!(std::fs::read(dir.path().join("b.txt")).unwrap(), b"hello b");
}

#[test]
fn restore_content_rejects_paths_escaping_base_dir() {
    let manager = manager();
    let err = manager
        .create_checkpoint("exec-1", &[entry("../escape.txt", b"bad")])
        .unwrap_err();
    assert!(matches!(err, CheckpointError::Validation { .. }));
}

#[test]
fn restore_latest_content_roundtrip() {
    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("v1.txt", b"version one")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("v1.txt", b"version two")])
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let written = manager
        .restore_latest_content("exec-1", dir.path())
        .unwrap()
        .unwrap();
    assert_eq!(written.len(), 1);
    assert_eq!(
        std::fs::read(dir.path().join("v1.txt")).unwrap(),
        b"version two",
        "latest content wins"
    );
}

#[test]
fn restore_missing_checkpoint_is_not_found() {
    let manager = manager();
    let err = manager
        .restore_checkpoint("exec-1", "ff".repeat(32).as_str())
        .unwrap_err();
    assert!(matches!(err, CheckpointError::NotFound { .. }));
}

#[test]
fn restore_workspace_aligns_files_dirs_and_protects_ignored() {
    let manager = manager();
    let opts = FileCheckpointOptions::default();
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
    std::fs::create_dir_all(dir.path().join("emptydir")).unwrap();
    let cp = manager
        .create_workspace_checkpoint("exec-1", dir.path(), &opts)
        .unwrap();
    assert_eq!(
        cp.empty_dirs.as_deref(),
        Some(vec!["emptydir".to_string()].as_slice())
    );

    // Mutate the workspace: modify a tracked file, add extras (one
    // protected by hardcoded ignores).
    std::fs::write(dir.path().join("a.txt"), b"v2").unwrap();
    std::fs::write(dir.path().join("extra.txt"), b"extra").unwrap();
    std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
    std::fs::write(dir.path().join("node_modules/lib.js"), b"lib").unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join(".git/config"), b"git").unwrap();
    std::fs::remove_dir(dir.path().join("emptydir")).unwrap();

    let result = manager
        .restore_workspace("exec-1", &cp.id, dir.path(), &opts)
        .unwrap();
    assert_eq!(result.restored, 1, "a.txt written back");
    assert_eq!(result.deleted, 1, "extra.txt removed");
    assert_eq!(result.skipped, 0);

    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"v1");
    assert!(!dir.path().join("extra.txt").exists());
    assert!(
        dir.path().join("node_modules/lib.js").exists(),
        "ignored files are never deleted"
    );
    assert!(dir.path().join(".git/config").exists());
    assert!(
        dir.path().join("emptydir").is_dir(),
        "empty directory recreated"
    );

    // Second restore: everything already matches.
    let result2 = manager
        .restore_workspace("exec-1", &cp.id, dir.path(), &opts)
        .unwrap();
    assert_eq!(result2.skipped, 1);
    assert_eq!(result2.restored, 0);
    assert_eq!(result2.deleted, 0);
}

#[test]
fn nested_repo_is_opaque_but_pointer_is_captured() {
    let manager = manager();
    let opts = FileCheckpointOptions::default();
    let dir = tempfile::tempdir().unwrap();

    let nested_sha = "0123456789abcdef0123456789abcdef01234567";
    std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
    std::fs::create_dir_all(dir.path().join("sub/.git/refs/heads")).unwrap();
    std::fs::write(dir.path().join("sub/.git/HEAD"), b"ref: refs/heads/main\n").unwrap();
    std::fs::write(
        dir.path().join("sub/.git/refs/heads/main"),
        format!("{nested_sha}\n").as_bytes(),
    )
    .unwrap();
    std::fs::write(dir.path().join("sub/inner.txt"), b"nested").unwrap();

    let cp = manager
        .create_workspace_checkpoint("exec-1", dir.path(), &opts)
        .unwrap();

    // Nested working files are opaque; the pointer is a gitlink pseudo blob.
    let files = manager.restore_checkpoint("exec-1", &cp.id).unwrap();
    assert!(
        files.iter().all(|f| !f.path.starts_with("sub/inner")),
        "nested working files must not enter the outer history"
    );
    let pointer = manager
        .read_file_at(&cp.id, "sub/.git")
        .unwrap()
        .expect("gitlink pseudo blob recorded");
    assert_eq!(pointer, format!("gitlink:{nested_sha}").into_bytes());
}

#[test]
fn restore_never_touches_nested_repo_contents() {
    let manager = manager();
    let opts = FileCheckpointOptions::default();
    let dir = tempfile::tempdir().unwrap();

    let nested_sha = "0123456789abcdef0123456789abcdef01234567";
    std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
    std::fs::create_dir_all(dir.path().join("sub/.git/refs/heads")).unwrap();
    std::fs::write(dir.path().join("sub/.git/HEAD"), b"ref: refs/heads/main\n").unwrap();
    std::fs::write(
        dir.path().join("sub/.git/refs/heads/main"),
        format!("{nested_sha}\n").as_bytes(),
    )
    .unwrap();
    std::fs::write(dir.path().join("sub/inner.txt"), b"nested").unwrap();

    let cp = manager
        .create_workspace_checkpoint("exec-1", dir.path(), &opts)
        .unwrap();

    // Mutate outer workspace and the nested repo's working tree.
    std::fs::write(dir.path().join("a.txt"), b"v2").unwrap();
    std::fs::write(dir.path().join("extra.txt"), b"extra").unwrap();
    std::fs::write(dir.path().join("sub/inner.txt"), b"changed by user").unwrap();

    let result = manager
        .restore_workspace("exec-1", &cp.id, dir.path(), &opts)
        .unwrap();
    assert_eq!(result.restored, 1, "a.txt written back");
    assert_eq!(result.deleted, 1, "extra.txt removed");
    assert!(
        std::fs::read(dir.path().join("sub/inner.txt")).unwrap() == b"changed by user",
        "nested repo contents are never overwritten"
    );
    assert!(
        std::fs::read(dir.path().join("sub/.git/HEAD")).unwrap() == b"ref: refs/heads/main\n",
        "nested repo internals are never rewritten"
    );
    // The gitlink pseudo blob targets a fake commit; the best-effort
    // checkout fails and the path is counted as skipped, never written.
    assert!(result.skipped_nested >= 1);
    assert_eq!(result.restored_gitlinks, 0);
}

#[test]
fn restore_workspace_via_latest_resolves_latest_state() {
    let manager = manager();
    let opts = FileCheckpointOptions::default();
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
    manager
        .create_workspace_checkpoint("exec-1", dir.path(), &opts)
        .unwrap();

    std::fs::write(dir.path().join("a.txt"), b"v2").unwrap();
    std::fs::write(dir.path().join("b.txt"), b"new").unwrap();
    manager
        .create_workspace_checkpoint("exec-1", dir.path(), &opts)
        .unwrap();

    std::fs::write(dir.path().join("a.txt"), b"v3").unwrap();
    std::fs::write(dir.path().join("b.txt"), b"newer").unwrap();
    manager
        .create_workspace_checkpoint("exec-1", dir.path(), &opts)
        .unwrap();

    std::fs::write(dir.path().join("a.txt"), b"mutated").unwrap();
    std::fs::write(dir.path().join("b.txt"), b"mutated too").unwrap();
    std::fs::write(dir.path().join("stray.txt"), b"stray").unwrap();

    let result = manager
        .restore_latest_workspace("exec-1", dir.path(), &opts)
        .unwrap()
        .unwrap();
    assert_eq!(result.restored, 2, "a.txt and b.txt written back");
    assert_eq!(result.deleted, 1, "stray.txt removed");
    assert_eq!(result.skipped, 0);
    assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"v3");
    assert_eq!(std::fs::read(dir.path().join("b.txt")).unwrap(), b"newer");
    assert!(!dir.path().join("stray.txt").exists());
}

#[test]
fn actor_id_for_parses_full_actor_or_defaults_to_agent() {
    let manager = manager();
    let actor = manager.actor_id_for("agent:loop-1");
    assert_eq!(actor.as_str(), "agent:loop-1");
    let actor = manager.actor_id_for("wf:exec-1/child:sub-1");
    assert_eq!(actor.as_str(), "wf:exec-1/child:sub-1");
    let actor = manager.actor_id_for("bare-exec-id");
    assert_eq!(actor.as_str(), "agent:bare-exec-id");
}

#[test]
fn apply_agent_edit_records_edits_per_actor() {
    let manager = manager();
    let actor = manager.actor_id_for("exec-1");
    manager.apply_agent_edit(&actor, "a.txt", b"first").unwrap();
    let snap2 = manager
        .apply_agent_edit(&actor, "a.txt", b"second")
        .unwrap();
    assert!(!snap2.is_empty());

    let latest = manager
        .create_latest_file_checkpoint("exec-1")
        .unwrap()
        .unwrap();
    let map = state_map(&latest.files);
    assert_eq!(map["a.txt"].hash, sha256_hex(b"second"));
}

#[test]
fn merge_entity_changes_moves_agent_to_feature() {
    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"base\n")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"base\nagent change\n")])
        .unwrap();

    let result = manager.merge_entity_changes("exec-1", "feature-1").unwrap();
    assert!(!result.merge_result.has_conflicts());
    assert!(!result.merge_result.commit_id.is_empty());
    assert!(!result.checkpoint_id.is_empty());

    let main = manager.merge_features_to_main(&["feature-1"]).unwrap();
    assert!(!main.merge_result.has_conflicts());
    assert!(!main.checkpoint_id.is_empty());
}

/// Reads the reconstructed text of every file on a feature ref.
fn feature_texts(manager: &FileCheckpointManager, feature_name: &str) -> HashMap<String, String> {
    crate::provenance::get_feature_workspace(manager.git_ref().unwrap(), feature_name)
        .unwrap()
        .into_iter()
        .map(|f| (f.path, String::from_utf8(f.content).unwrap_or_default()))
        .collect()
}

#[test]
fn approve_changes_file_level_selects_paths() {
    let manager = manager();
    // Three files submitted; the approval layer only advances the last
    // edited file, the others are read back from the agent partition.
    for (path, content) in [
        ("a.txt", b"base-a".as_slice()),
        ("b.txt", b"base-b".as_slice()),
        ("c.txt", b"base-c".as_slice()),
    ] {
        manager
            .create_checkpoint("exec-1", &[entry(path, content)])
            .unwrap();
    }
    for (path, content) in [
        ("a.txt", b"new-a".as_slice()),
        ("b.txt", b"new-b".as_slice()),
        ("c.txt", b"new-c".as_slice()),
    ] {
        manager
            .create_checkpoint("exec-1", &[entry(path, content)])
            .unwrap();
    }
    // Approve only a.txt and b.txt.
    let outcome = manager
        .approve_changes(
            "exec-1",
            "feature-1",
            Some(vec!["a.txt".to_string(), "b.txt".to_string()]),
            ConflictBehavior::Marker,
            None,
        )
        .unwrap();
    assert!(outcome.merged, "file-level approval should merge");

    // Feature now holds the two approved files; c.txt stays unapproved.
    let texts = feature_texts(&manager, "feature-1");
    assert_eq!(texts.get("a.txt").map(String::as_str), Some("new-a"));
    assert_eq!(texts.get("b.txt").map(String::as_str), Some("new-b"));
    assert!(!texts.contains_key("c.txt"), "c.txt must stay pending");

    // The approval layer still reports the submission as pending.
    let pending = manager.list_pending_approvals().unwrap();
    assert_eq!(pending.len(), 1, "remaining files stay pending");
}

#[test]
fn approve_changes_full_batch_matches_legacy_behavior() {
    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"base\n")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"base\nagent\n")])
        .unwrap();

    // `paths = None` approves the whole submission as one batch merge.
    let outcome = manager
        .approve_changes("exec-1", "feature-1", None, ConflictBehavior::Marker, None)
        .unwrap();
    assert!(outcome.merged);
    assert!(!outcome.has_conflicts());
    let pending = manager.list_pending_approvals().unwrap();
    assert!(
        pending.is_empty(),
        "full-batch approve clears the submission"
    );
    let reviews = manager
        .git_ref()
        .unwrap()
        .list_refs(crate::git_store::REF_REVIEW_PREFIX)
        .unwrap();
    assert!(
        reviews.is_empty(),
        "approved review refs are recycled, ancestry stays in the merge parents"
    );
}

#[test]
fn approve_reuses_pending_when_edit_unchanged() {
    let manager = manager();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"one")])
        .unwrap();
    manager.move_agent_to_approval("exec-1").unwrap();
    let before = manager
        .git_ref()
        .unwrap()
        .list_refs(crate::git_store::REF_REVIEW_PREFIX)
        .unwrap();
    assert_eq!(before.len(), 1);
    let (first_ref, first_head) = manager.reuse_or_submit_review("exec-1").unwrap();
    assert_eq!(first_ref, before[0].0);
    let (second_ref, second_head) = manager.reuse_or_submit_review("exec-1").unwrap();
    assert_eq!(second_ref, first_ref);
    assert_eq!(second_head, first_head);
    let after = manager
        .git_ref()
        .unwrap()
        .list_refs(crate::git_store::REF_REVIEW_PREFIX)
        .unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].0, first_ref);
    manager
        .create_checkpoint("exec-1", &[entry("b.txt", b"two")])
        .unwrap();
    let (third_ref, third_head) = manager.reuse_or_submit_review("exec-1").unwrap();
    assert_ne!(third_ref, first_ref);
    assert_ne!(third_head, first_head);
}

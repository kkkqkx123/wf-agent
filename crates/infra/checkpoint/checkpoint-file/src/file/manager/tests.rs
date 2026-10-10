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

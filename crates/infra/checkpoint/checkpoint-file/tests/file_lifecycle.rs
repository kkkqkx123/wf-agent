//! Integration tests: file-checkpoint lifecycle on the Git model.
//!
//! Scenarios covered:
//!
//! - edit → undo → redo round-trip through the actor edit ref;
//! - redo history survives a manager reopen (durable redo stack);
//! - a fresh commit clears the redo history it superseded;
//! - per-actor isolation: undoing one actor's line never touches another's;
//! - checkpoint create → restore returns the recorded per-file state;
//! - GC keeps reachable history restorable;
//! - store statistics report reachable commits, refs and last GC time.

use std::path::Path;

use checkpoint_base::actor::id::{ActorId, ActorKind};
use checkpoint_file::file::{FileCheckpointManager, FileContentEntry};
use checkpoint_file::gc::GcRetention;
use wf_types::config::file_checkpoint::{FailureBehavior, FileCheckpointConfig};
use wf_types::Id;

/// A config binding a manager to `dir` as its workspace root, with the store
/// kept inside the tempdir so tests never touch the real per-user data dir.
fn config_for(dir: &Path) -> FileCheckpointConfig {
    FileCheckpointConfig {
        enabled: true,
        workspace_root: Some(dir.to_string_lossy().to_string()),
        storage: None,
        store_in_workspace: Some(true),
        failure_behavior: FailureBehavior::Error,
        ..FileCheckpointConfig::default()
    }
}

fn manager_for(dir: &Path) -> FileCheckpointManager {
    FileCheckpointManager::open_from_config(&config_for(dir)).expect("manager opens")
}

fn actor(id: &str) -> ActorId {
    ActorId::new(ActorKind::Agent, &[Id::from(id.to_string())]).expect("actor id valid")
}

fn entry(path: &str, content: &[u8]) -> FileContentEntry {
    FileContentEntry::new(path, content.to_vec())
}

/// Current content of one file in an actor's reconstructed workspace.
fn content_of(manager: &FileCheckpointManager, entity: &str, path: &str) -> Option<Vec<u8>> {
    manager
        .get_actor_workspace(entity)
        .expect("workspace reads")
        .into_iter()
        .find(|file| file.path == path)
        .map(|file| file.content)
}

#[test]
fn edit_undo_redo_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager_for(dir.path());
    let agent = actor("loop-1");

    manager.apply_agent_edit(&agent, "a.txt", b"v1").unwrap();
    manager.apply_agent_edit(&agent, "a.txt", b"v2").unwrap();
    assert_eq!(content_of(&manager, agent.as_str(), "a.txt").as_deref(), Some(&b"v2"[..]));

    manager.undo_edit(agent.as_str()).unwrap();
    assert_eq!(content_of(&manager, agent.as_str(), "a.txt").as_deref(), Some(&b"v1"[..]));
    assert!(manager.can_redo(agent.as_str()).unwrap());

    manager.redo_edit(agent.as_str()).unwrap();
    assert_eq!(content_of(&manager, agent.as_str(), "a.txt").as_deref(), Some(&b"v2"[..]));
    assert!(!manager.can_redo(agent.as_str()).unwrap());
}

#[test]
fn redo_history_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let agent = actor("loop-1");

    {
        let manager = manager_for(dir.path());
        manager.apply_agent_edit(&agent, "a.txt", b"v1").unwrap();
        manager.apply_agent_edit(&agent, "a.txt", b"v2").unwrap();
        manager.undo_edit(agent.as_str()).unwrap();
        assert!(manager.can_redo(agent.as_str()).unwrap());
    }

    // A fresh manager over the same store must still offer the redo.
    let reopened = manager_for(dir.path());
    assert_eq!(
        content_of(&reopened, agent.as_str(), "a.txt").as_deref(),
        Some(&b"v1"[..]),
        "undo state must persist across reopen"
    );
    assert!(
        reopened.can_redo(agent.as_str()).unwrap(),
        "redo history must survive reopen"
    );
    reopened.redo_edit(agent.as_str()).unwrap();
    assert_eq!(
        content_of(&reopened, agent.as_str(), "a.txt").as_deref(),
        Some(&b"v2"[..])
    );
    assert!(!reopened.can_redo(agent.as_str()).unwrap());
}

#[test]
fn new_commit_clears_superseded_redo() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager_for(dir.path());
    let agent = actor("loop-1");

    manager.apply_agent_edit(&agent, "a.txt", b"v1").unwrap();
    manager.apply_agent_edit(&agent, "a.txt", b"v2").unwrap();
    manager.undo_edit(agent.as_str()).unwrap();
    assert!(manager.can_redo(agent.as_str()).unwrap());

    manager.apply_agent_edit(&agent, "a.txt", b"v3").unwrap();
    assert!(
        !manager.can_redo(agent.as_str()).unwrap(),
        "a new commit supersedes the undone history"
    );
    assert!(manager.redo_edit(agent.as_str()).is_err());
    assert_eq!(content_of(&manager, agent.as_str(), "a.txt").as_deref(), Some(&b"v3"[..]));
}

#[test]
fn undo_isolated_per_actor() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager_for(dir.path());
    let a = actor("a");
    let b = actor("b");

    manager.apply_agent_edit(&a, "a.txt", b"a1").unwrap();
    manager.apply_agent_edit(&b, "b.txt", b"b1").unwrap();
    manager.apply_agent_edit(&a, "a.txt", b"a2").unwrap();

    manager.undo_edit(a.as_str()).unwrap();
    assert_eq!(content_of(&manager, a.as_str(), "a.txt").as_deref(), Some(&b"a1"[..]));
    assert_eq!(
        content_of(&manager, b.as_str(), "b.txt").as_deref(),
        Some(&b"b1"[..]),
        "undoing one actor must not touch another's line"
    );
    assert!(!manager.can_redo(b.as_str()).unwrap());
}

#[test]
fn checkpoint_restore_returns_recorded_state() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager_for(dir.path());

    let first = manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"one")])
        .unwrap();
    manager
        .create_checkpoint("exec-1", &[entry("a.txt", b"two")])
        .unwrap();

    let restored = manager.restore_checkpoint("exec-1", &first.id).unwrap();
    let a = restored
        .iter()
        .find(|file| file.path == "a.txt")
        .expect("a.txt restored");
    assert_eq!(a.hash, checkpoint_file::file::util::sha256_hex(b"one"));
}

#[test]
fn gc_keeps_reachable_history_restorable() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager_for(dir.path());
    let agent = actor("loop-1");

    manager.apply_agent_edit(&agent, "a.txt", b"v1").unwrap();
    manager.apply_agent_edit(&agent, "b.txt", b"keep").unwrap();

    manager.run_gc(GcRetention::default()).unwrap();
    assert_eq!(
        content_of(&manager, agent.as_str(), "b.txt").as_deref(),
        Some(&b"keep"[..]),
        "reachable history must stay restorable after GC"
    );
}

#[test]
fn store_stats_report_commits_and_last_gc() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager_for(dir.path());
    let agent = actor("loop-1");

    manager.apply_agent_edit(&agent, "a.txt", b"v1").unwrap();
    let before = manager.store_stats().unwrap();
    assert!(before.commits >= 1);
    assert!(before.refs >= 1);
    assert!(before.store_bytes > 0);
    assert!(before.last_gc_ms.is_none());

    manager.run_gc(GcRetention::default()).unwrap();
    let after = manager.store_stats().unwrap();
    assert!(
        after.last_gc_ms.is_some(),
        "GC must record its completion time"
    );
}

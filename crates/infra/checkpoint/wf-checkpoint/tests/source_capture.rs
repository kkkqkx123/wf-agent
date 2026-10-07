//! Integration tests: modification-source capture on the Git model.
//!
//! Scenarios covered:
//!
//! - script-change capture: a workspace diff collected over the
//!   `PathPolicy.allowed_write` scope produces exactly one atomic commit
//!   on the executing actor's edit ref;
//! - tool-report capture: in-memory bytes commit without a disk re-read
//!   (one tool call is one commit, multi-file batches stay atomic);
//! - manual capture: worktree-dirty files that do not match agent-owned
//!   content land on the human ref and are never auto-merged; agent
//!   self-writes are skipped by content comparison;
//! - the `ManualChangeService` poll routes external edits onto the human
//!   ref; the source index rebuilds fully from the commit graph.

use std::path::Path;
use std::time::Duration;

use wf_checkpoint::actor::id::{ActorId, ActorKind};
use wf_checkpoint::file::FileCheckpointManager;
use wf_checkpoint::provenance::{get_actor_workspace, list_changes_by_path};
use wf_checkpoint::scan::ScanConfig;
use wf_checkpoint::script_capture::WorkspaceChangeCollector;
use wf_checkpoint::watcher::{FileChangeKind, FileChangeRecord, ManualChangeService};
use wf_types::config::file_checkpoint::{FailureBehavior, FileCheckpointConfig};
use wf_types::Id;

/// A manager bound to `dir` as its workspace root.
fn manager_for(dir: &Path) -> FileCheckpointManager {
    let config = FileCheckpointConfig {
        enabled: true,
        workspace_root: Some(dir.to_string_lossy().to_string()),
        storage: None,
        failure_behavior: FailureBehavior::Error,
        ..FileCheckpointConfig::default()
    };
    FileCheckpointManager::open_from_config(&config).expect("manager opens")
}

fn actor(kind: ActorKind, id: &str) -> ActorId {
    ActorId::new(kind, &[Id::from(id.to_string())]).expect("actor id valid")
}

fn stores_of(
    manager: &FileCheckpointManager,
) -> (
    std::sync::Arc<wf_checkpoint::git_store::GitStore>,
    std::sync::Arc<wf_checkpoint::storage::SqliteStorage>,
) {
    (
        manager.git_store().unwrap().clone(),
        manager.storage().unwrap().clone(),
    )
}

/// Script-change capture: one run is exactly one atomic commit on the
/// actor's edit ref (add/modify/delete + binary).
#[test]
fn script_capture_produces_single_atomic_commit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("a.txt"), b"v1\n").unwrap();
    std::fs::write(root.join("gone.txt"), b"to-be-deleted").unwrap();

    let manager = manager_for(root);
    let script = actor(ActorKind::Agent, "script-run-1");

    // Scope = `PathPolicy.allowed_write` ("." = whole workspace).
    let collector = manager
        .collector_for(&[".".to_string()])
        .expect("scope present");
    let before = collector.capture().unwrap();

    // The "script" modifies a.txt, writes a binary file and deletes gone.txt.
    std::fs::write(root.join("a.txt"), b"v1\nv2\n").unwrap();
    std::fs::write(root.join("b.bin"), [0x00, 0x01, 0x02, 0x03]).unwrap();
    std::fs::remove_file(root.join("gone.txt")).unwrap();

    let after = collector.capture().unwrap();
    let changes = WorkspaceChangeCollector::diff(&before, &after);
    assert_eq!(changes.len(), 3);

    let (git, storage) = stores_of(&manager);
    let head_before = git
        .read_ref(&wf_checkpoint::git_store::edit_ref_for_actor(
            script.as_str(),
        ))
        .unwrap();
    let applied = manager
        .apply_workspace_changes(&script, root, &changes, manager.failure_behavior())
        .expect("changes applied");
    assert_eq!(applied, 3);
    let head_after = git
        .read_ref(&wf_checkpoint::git_store::edit_ref_for_actor(
            script.as_str(),
        ))
        .unwrap()
        .expect("script run commits");
    assert_ne!(head_before, Some(head_after.clone()));

    // Exactly one new commit, carrying all three files.
    let commit = git.read_commit(&head_after).unwrap();
    assert_eq!(commit.parents.len(), head_before.map(|_| 1).unwrap_or(0));
    let files = git.tree_to_bytes(&commit.tree).unwrap();
    assert_eq!(files["a.txt"], b"v1\nv2\n");
    assert_eq!(files["b.bin"], [0x00, 0x01, 0x02, 0x03]);
    assert!(!files.contains_key("gone.txt"));

    let workspace = get_actor_workspace(&git, script.as_str()).unwrap();
    let by_path: std::collections::HashMap<_, _> = workspace
        .iter()
        .map(|f| (f.path.as_str(), &f.content))
        .collect();
    assert_eq!(by_path.get("a.txt").unwrap().as_slice(), b"v1\nv2\n");
    assert!(
        !by_path.contains_key("gone.txt"),
        "deleted file must be absent from the actor workspace"
    );
    let _ = storage;
}

/// Out-of-workspace prefixes are excluded from the capture scope (scripts
/// writing to /tmp etc. are not tracked) — an empty scope disables capture.
#[test]
fn script_capture_scope_excludes_outside_prefixes() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), b"x").unwrap();
    std::fs::write(outside.path().join("tmp.txt"), b"y").unwrap();

    let manager = manager_for(dir.path());
    let collector = manager.collector_for(&[outside.path().to_string_lossy().to_string()]);
    assert!(
        collector.is_none(),
        "an all-outside scope must disable capture"
    );
}

/// Tool-report capture: in-memory bytes commit without touching the disk
/// (one tool call is one commit even for multi-file batches).
#[test]
fn tool_report_commits_memory_bytes_without_disk_reread() {
    use wf_checkpoint::file::actor::{PreciseFileEvent, PreciseFileEventKind};

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let manager = manager_for(root);
    let agent = actor(ActorKind::Agent, "tool-1");

    // The worktree files do not exist on disk: the tool-captured bytes are
    // authoritative and no disk re-read happens.
    let events = vec![
        PreciseFileEvent::new(root.join("a.txt"), PreciseFileEventKind::Created)
            .with_content(b"from-tool-a".to_vec(), "hash-a".to_string()),
        PreciseFileEvent::new(root.join("sub/b.txt"), PreciseFileEventKind::Created)
            .with_content(b"from-tool-b".to_vec(), "hash-b".to_string()),
    ];
    let stats = manager
        .apply_precise_file_events(&agent, root, &events, FailureBehavior::Error)
        .unwrap();
    assert_eq!(stats.applied, 2);

    let (git, _) = stores_of(&manager);
    let head = git
        .read_ref(&wf_checkpoint::git_store::edit_ref_for_actor(
            agent.as_str(),
        ))
        .unwrap()
        .expect("tool call commits");
    let commit = git.read_commit(&head).unwrap();
    // One tool call is one commit.
    assert_eq!(commit.parents.len(), 0);
    let files = git.tree_to_bytes(&commit.tree).unwrap();
    assert_eq!(files["a.txt"], b"from-tool-a");
    assert_eq!(files["sub/b.txt"], b"from-tool-b");
}

/// Manual capture: agent-owned content is skipped, human edits land on the
/// human ref (never auto-merged), unlinks use delete semantics.
#[test]
fn manual_changes_skip_agent_content_and_record_human_edits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let manager = manager_for(root);
    let agent = actor(ActorKind::Agent, "loop-1");
    let (git, storage) = stores_of(&manager);

    // Agent writes a.txt; the same bytes arriving as a watcher event are
    // agent-owned and skipped.
    manager
        .apply_agent_edit(&agent, "a.txt", b"agent-v1")
        .unwrap();
    std::fs::write(root.join("a.txt"), b"agent-v1").unwrap();

    let self_write =
        FileChangeRecord::new(root.join("a.txt"), FileChangeKind::Add, wf_common::now());
    let applied = manager
        .process_manual_changes(&[self_write])
        .expect("no error on skipped self-write");
    assert_eq!(applied, 0, "agent-owned content must be skipped");
    assert!(
        git.read_ref(wf_checkpoint::git_store::REF_HUMAN)
            .unwrap()
            .is_none(),
        "no human commit may exist yet"
    );

    // Human edits b.txt: recorded on the human ref.
    std::fs::write(root.join("b.txt"), b"human-edit").unwrap();
    let human = FileChangeRecord::new(root.join("b.txt"), FileChangeKind::Change, wf_common::now());
    let applied = manager
        .process_manual_changes(&[human])
        .expect("human edit applied");
    assert_eq!(applied, 1);
    let human_head = git
        .read_ref(wf_checkpoint::git_store::REF_HUMAN)
        .unwrap()
        .expect("human commit exists");
    let manual_for_b = list_changes_by_path(&git, &storage, "b.txt", None).unwrap();
    assert!(
        manual_for_b.iter().any(|c| c.snapshot_id == human_head),
        "human edit must be recorded on the human ref"
    );
    // The human line never leaks into the actor line.
    let actor_files = get_actor_workspace(&git, agent.as_str()).unwrap();
    assert!(!actor_files.iter().any(|f| f.path == "b.txt"));

    // Unlink: human delete semantics (absent from the human tree).
    std::fs::remove_file(root.join("b.txt")).unwrap();
    let unlink =
        FileChangeRecord::new(root.join("b.txt"), FileChangeKind::Unlink, wf_common::now());
    let applied = manager
        .process_manual_changes(&[unlink])
        .expect("unlink applied");
    assert_eq!(applied, 1);
    let human_head = git
        .read_ref(wf_checkpoint::git_store::REF_HUMAN)
        .unwrap()
        .unwrap();
    let files = git
        .tree_to_bytes(&git.read_commit(&human_head).unwrap().tree)
        .unwrap();
    assert!(!files.contains_key("b.txt"));
}

/// End-to-end: the poll routes external edits onto the human ref, and
/// agent-owned content is not double-recorded.
#[tokio::test]
async fn manual_change_service_routes_external_edits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let manager = manager_for(root);
    let (git, storage) = stores_of(&manager);

    let mut service =
        ManualChangeService::start(manager.clone(), root, ScanConfig::default(), 50, 50)
            .expect("service starts");

    std::fs::write(root.join("human.txt"), b"hello watcher").unwrap();
    wait_until(
        || {
            let changes = list_changes_by_path(&git, &storage, "human.txt", None).unwrap();
            changes.iter().any(|c| c.source == "human")
        },
        10_000,
    )
    .await;

    // Agent-owned content arriving on disk is skipped: the poll compares
    // against tracked ref content, so it never lands on the human ref.
    manager
        .apply_agent_edit(
            &actor(ActorKind::Agent, "loop-watch"),
            "agent-write.txt",
            b"agent",
        )
        .unwrap();
    std::fs::write(root.join("agent-write.txt"), b"agent").unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let agent_changes = list_changes_by_path(&git, &storage, "agent-write.txt", None).unwrap();
    assert!(
        agent_changes.iter().all(|c| c.source != "human"),
        "agent-owned content must not be recorded as human"
    );

    service.stop().await;
}

/// The source index rebuilds fully from the commit graph after total loss.
#[test]
fn source_index_rebuilds_from_graph() {
    let dir = tempfile::tempdir().unwrap();
    let manager = manager_for(dir.path());
    let agent = actor(ActorKind::Agent, "loop-9");
    let (git, storage) = stores_of(&manager);

    manager.apply_agent_edit(&agent, "a.txt", b"one").unwrap();
    manager.apply_agent_edit(&agent, "b.txt", b"two").unwrap();
    assert!(!storage
        .find_commits_by_actor(agent.as_str(), 0)
        .unwrap()
        .is_empty());

    storage.clear_source_index().unwrap();
    assert!(storage
        .find_commits_by_actor(agent.as_str(), 0)
        .unwrap()
        .is_empty());

    // Queries fall back to the graph while the index is empty. The second
    // commit's tree still carries a.txt, so per-file summaries total three.
    let via_graph = wf_checkpoint::provenance::list_changes_by_actor(
        &git,
        &storage,
        agent.as_str(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(via_graph.len(), 3);

    let rebuilt = manager.rebuild_source_index().unwrap();
    assert_eq!(rebuilt, 2);
    assert_eq!(
        storage
            .find_commits_by_actor(agent.as_str(), 0)
            .unwrap()
            .len(),
        2
    );
}

async fn wait_until(mut cond: impl FnMut() -> bool, timeout_ms: u64) {
    assert!(
        wf_common::poll_until(
            Duration::from_millis(50),
            Duration::from_millis(timeout_ms),
            || {
                let matched = cond();
                async move { matched }
            },
        )
        .await,
        "condition not met within {timeout_ms}ms"
    );
}

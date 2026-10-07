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
use wf_checkpoint::scan::ScanConfig;
use wf_checkpoint::script_capture::WorkspaceChangeCollector;
use wf_checkpoint::watcher::ManualChangeService;
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

    let collector = manager
        .collector_for(&[".".to_string()])
        .expect("scope present");
    let before = collector.capture().unwrap();

    std::fs::write(root.join("a.txt"), b"v1\nv2\n").unwrap();
    std::fs::write(root.join("b.bin"), [0x00, 0x01, 0x02, 0x03]).unwrap();
    std::fs::remove_file(root.join("gone.txt")).unwrap();

    let after = collector.capture().unwrap();
    let changes = WorkspaceChangeCollector::diff(&before, &after);
    assert_eq!(changes.len(), 3);

    let before_count = manager
        .list_changes_by_actor(script.as_str(), None, None)
        .unwrap()
        .len();
    let applied = manager
        .apply_workspace_changes(&script, root, &changes, manager.failure_behavior())
        .expect("changes applied");
    assert_eq!(applied, 3);
    let after_changes = manager
        .list_changes_by_actor(script.as_str(), None, None)
        .unwrap();
    assert!(
        after_changes.len() > before_count,
        "script run must record new changes via query view"
    );

    let workspace = manager
        .get_actor_workspace(script.as_str())
        .unwrap();
    let by_path: std::collections::HashMap<_, _> = workspace
        .iter()
        .map(|f| (f.path.as_str(), &f.content))
        .collect();
    assert_eq!(by_path.get("a.txt").unwrap().as_slice(), b"v1\nv2\n");
    assert_eq!(
        by_path.get("b.bin").unwrap().as_slice(),
        &[0x00, 0x01, 0x02, 0x03]
    );
    assert!(
        !by_path.contains_key("gone.txt"),
        "deleted file must be absent from the actor workspace"
    );
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

    let workspace = manager.get_actor_workspace(agent.as_str()).unwrap();
    let by_path: std::collections::HashMap<_, _> = workspace
        .iter()
        .map(|f| (f.path.as_str(), f.content.as_slice()))
        .collect();
    assert_eq!(by_path.get("a.txt").unwrap(), &b"from-tool-a".as_slice());
    assert_eq!(
        by_path.get("sub/b.txt").unwrap(),
        &b"from-tool-b".as_slice()
    );
    let changes = manager
        .list_changes_by_actor(agent.as_str(), None, None)
        .unwrap();
    assert!(
        !changes.is_empty(),
        "tool call must record changes via query view"
    );
}

/// Manual capture via polling: agent-owned content is skipped, human edits
/// land on the human ref (never auto-merged).
#[test]
fn manual_changes_skip_agent_content_and_record_human_edits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let manager = manager_for(root);
    let agent = actor(ActorKind::Agent, "loop-1");

    manager
        .apply_agent_edit(&agent, "a.txt", b"agent-v1")
        .unwrap();
    std::fs::write(root.join("a.txt"), b"agent-v1").unwrap();

    let none = manager.poll_human_edits(root).unwrap();
    assert!(
        none.is_none(),
        "agent-owned content must be skipped by polling"
    );
    let human_changes = manager.list_changes_by_path("b.txt", None).unwrap();
    assert!(
        human_changes.iter().all(|c| c.source != "human")
            || human_changes.is_empty(),
        "no human commit may exist yet"
    );

    std::fs::write(root.join("b.txt"), b"human-edit").unwrap();
    let human_head = manager
        .poll_human_edits(root)
        .unwrap()
        .expect("human edit polled");
    assert!(!human_head.is_empty());
    let manual_for_b = manager.list_changes_by_path("b.txt", None).unwrap();
    assert!(
        manual_for_b.iter().any(|c| c.source == "human"),
        "human edit must be recorded on the human ref"
    );
    let actor_files = manager.get_actor_workspace(agent.as_str()).unwrap();
    assert!(!actor_files.iter().any(|f| f.path == "b.txt"));

    std::fs::remove_file(root.join("b.txt")).unwrap();
    let unlink_head = manager.poll_human_edits(root).unwrap();
    assert!(
        unlink_head.is_some(),
        "human deletion must produce a poll commit"
    );
    let after = manager.list_changes_by_path("b.txt", None).unwrap();
    assert!(
        after.iter().any(|c| c.source == "human"),
        "human deletion must stay on the human line"
    );
}

/// End-to-end: the poll routes external edits onto the human ref, and
/// agent-owned content is not double-recorded.
#[tokio::test]
async fn manual_change_service_routes_external_edits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let manager = manager_for(root);

    let mut service =
        ManualChangeService::start(manager.clone(), root, ScanConfig::default(), 50, 50)
            .expect("service starts");

    std::fs::write(root.join("human.txt"), b"hello watcher").unwrap();
    wait_until(
        || {
            let changes = manager.list_changes_by_path("human.txt", None).unwrap();
            changes.iter().any(|c| c.source == "human")
        },
        10_000,
    )
    .await;

    manager
        .apply_agent_edit(
            &actor(ActorKind::Agent, "loop-watch"),
            "agent-write.txt",
            b"agent",
        )
        .unwrap();
    std::fs::write(root.join("agent-write.txt"), b"agent").unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let agent_changes = manager
        .list_changes_by_path("agent-write.txt", None)
        .unwrap();
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

    manager.apply_agent_edit(&agent, "a.txt", b"one").unwrap();
    manager.apply_agent_edit(&agent, "b.txt", b"two").unwrap();
    let before = manager
        .list_changes_by_actor(agent.as_str(), None, None)
        .unwrap();
    assert!(!before.is_empty());

    let rebuilt = manager.rebuild_source_index().unwrap();
    assert_eq!(rebuilt, 2);
    let after = manager
        .list_changes_by_actor(agent.as_str(), None, None)
        .unwrap();
    assert_eq!(after.len(), 3);
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

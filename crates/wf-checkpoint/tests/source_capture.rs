//! Integration tests: modification-source capture.
//!
//! Scenarios covered:
//!
//! - script-change capture: a workspace diff (add/modify/delete + binary
//!   content) collected over the `PathPolicy.allowed_write` scope is
//!   attributed to the executing actor partition;
//! - manual capture: watcher records are hashed and compared against the
//!   recent-agent-writes registry — agent self-writes are skipped, human
//!   edits land in the manual partition, unlinks use delete semantics;
//! - the end-to-end `ManualChangeService`: external file edits are routed
//!   into the manual partition by the real file watcher.

use std::path::Path;
use std::time::{Duration, Instant};

use wf_checkpoint::actor_id::{ActorId, ActorKind};
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

/// Script-change capture: a workspace diff (add/modify/delete + binary) is
/// attributed to the executing actor partition.
#[test]
fn script_capture_attributes_changes_to_actor_partition() {
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

    let applied = manager
        .apply_workspace_changes(&script, root, &changes, manager.failure_behavior())
        .expect("changes applied");
    assert_eq!(applied, 3);

    let workspace = get_actor_workspace(manager.storage().unwrap(), script.as_str()).unwrap();
    let by_path: std::collections::HashMap<_, _> = workspace
        .iter()
        .map(|f| (f.path.as_str(), &f.content))
        .collect();
    assert_eq!(by_path.get("a.txt").unwrap().as_slice(), b"v1\nv2\n");
    assert_eq!(
        by_path.get("b.bin").unwrap().as_slice(),
        [0x00, 0x01, 0x02, 0x03]
    );
    // Deletion semantics: empty content in the partition + deletion
    // projection marker (explicit delete).
    assert_eq!(
        by_path.get("gone.txt").unwrap().as_slice(),
        b"",
        "deleted file holds an empty-content marker"
    );
    assert!(
        manager.deleted_files(script.as_str()).contains("gone.txt"),
        "deleted file must be in the deletion projection marker"
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

/// Manual capture: agent self-writes are skipped (hash comparison), human
/// edits land in the manual partition, unlinks use delete semantics.
#[test]
fn manual_changes_skip_agent_writes_and_record_human_edits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let manager = manager_for(root);
    let agent = actor(ActorKind::Agent, "loop-1");

    // Agent writes a.txt; the watcher event for the same content must be
    // skipped (deterministic hash comparison, outside the grace window).
    manager
        .apply_agent_edit(&agent, "a.txt", b"agent-v1")
        .unwrap();
    std::fs::write(root.join("a.txt"), b"agent-v1").unwrap();
    std::thread::sleep(Duration::from_millis(120));

    let self_write = FileChangeRecord {
        path: root.join("a.txt"),
        kind: FileChangeKind::Add,
        timestamp: wf_common::now(),
    };
    let applied = manager
        .process_manual_changes(&[self_write])
        .expect("no error on skipped self-write");
    assert_eq!(applied, 0, "agent self-write must be skipped");
    let manual_for_a = list_changes_by_path(manager.storage().unwrap(), "a.txt", None).unwrap();
    assert!(
        !manual_for_a.iter().any(|c| c.source == "manual"),
        "agent write must not be recorded as manual"
    );

    // Human edits b.txt: recorded into the manual partition.
    std::fs::write(root.join("b.txt"), b"human-edit").unwrap();
    let human = FileChangeRecord {
        path: root.join("b.txt"),
        kind: FileChangeKind::Change,
        timestamp: wf_common::now(),
    };
    let applied = manager
        .process_manual_changes(&[human])
        .expect("human edit applied");
    assert_eq!(applied, 1);
    let manual_for_b = list_changes_by_path(manager.storage().unwrap(), "b.txt", None).unwrap();
    assert!(
        manual_for_b.iter().any(|c| c.source == "manual"),
        "human edit must be recorded as manual"
    );

    // Unlink: manual delete semantics (empty content in the manual partition).
    std::fs::remove_file(root.join("b.txt")).unwrap();
    let unlink = FileChangeRecord {
        path: root.join("b.txt"),
        kind: FileChangeKind::Unlink,
        timestamp: wf_common::now(),
    };
    let applied = manager
        .process_manual_changes(&[unlink])
        .expect("unlink applied");
    assert_eq!(applied, 1);
}

/// End-to-end: the real watcher routes external edits into the manual
/// partition, and agent self-writes are not double-recorded.
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
            let changes =
                list_changes_by_path(manager.storage().unwrap(), "human.txt", None).unwrap();
            changes.iter().any(|c| c.source == "manual")
        },
        10_000,
    )
    .await;

    // An agent self-write arriving through the watcher is skipped: the
    // manager registered the absolute path + hash, so the pump's hash
    // comparison recognizes it as the agent's own write.
    manager
        .apply_agent_edit(
            &actor(ActorKind::Agent, "loop-watch"),
            "agent-write.txt",
            b"agent",
        )
        .unwrap();
    std::fs::write(root.join("agent-write.txt"), b"agent").unwrap();
    // Wait for the watcher to see the event and the pump to skip it.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let agent_changes =
        list_changes_by_path(manager.storage().unwrap(), "agent-write.txt", None).unwrap();
    assert!(
        !agent_changes.iter().any(|c| c.source == "manual"),
        "agent self-write must not be double-recorded as manual"
    );

    service.stop().await;
}

async fn wait_until(mut cond: impl FnMut() -> bool, timeout_ms: u64) {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not met within {timeout_ms}ms");
}

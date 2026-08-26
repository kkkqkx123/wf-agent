//! CLI smoke tests: the `wf-checkpoint` binary lists and dumps
//! checkpoints from a persisted Sqlite store. `CARGO_BIN_EXE_wf-checkpoint`
//! points at the freshly built binary.

use std::process::Command;
use std::sync::Arc;

use wf_checkpoint::state::CheckpointStateManager;
use wf_checkpoint::state::StorageBackedStateManager;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::BaseCheckpointCore;

type Envelope = BaseCheckpointCore<serde_json::Value, serde_json::Value>;

fn make_envelope(id: &str, timestamp: i64) -> Envelope {
    BaseCheckpointCore {
        id: id.to_string(),
        r#type: None,
        base_checkpoint_id: None,
        previous_checkpoint_id: None,
        delta: None,
        snapshot: Some(serde_json::json!({ "state": "hello", "value": 42 })),
        timestamp: Some(timestamp),
        metadata: None,
        format_version: None,
    }
}

#[tokio::test]
async fn cli_lists_and_dumps_checkpoints() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.db");
    let db = db.to_str().unwrap();

    let manager = StorageBackedStateManager::<Envelope>::new(Arc::new(
        StorageBackend::new_sqlite(db, "checkpoints").await.unwrap(),
    ));
    manager
        .save(&make_envelope("cp-1", 1000), "workflow", "exec-1")
        .await
        .unwrap();
    manager
        .save(&make_envelope("cp-2", 2000), "workflow", "exec-1")
        .await
        .unwrap();

    // list <execution>
    let out = Command::new(env!("CARGO_BIN_EXE_wf-checkpoint"))
        .args(["--db", db, "list", "exec-1"])
        .output()
        .expect("cli list runs");
    assert!(out.status.success(), "list exits 0: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cp-1"), "list shows cp-1: {stdout}");
    assert!(stdout.contains("cp-2"), "list shows cp-2: {stdout}");
    assert!(stdout.contains("exec-1"), "list shows the execution id");

    // dump <checkpoint-id> yields readable JSON carrying the snapshot.
    let out = Command::new(env!("CARGO_BIN_EXE_wf-checkpoint"))
        .args(["--db", db, "dump", "cp-2"])
        .output()
        .expect("cli dump runs");
    assert!(out.status.success(), "dump exits 0: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("dump is JSON");
    assert_eq!(value["id"], serde_json::json!("cp-2"));
    assert_eq!(value["snapshot"]["value"], serde_json::json!(42));

    // dump of a missing checkpoint exits non-zero with a message.
    let out = Command::new(env!("CARGO_BIN_EXE_wf-checkpoint"))
        .args(["--db", db, "dump", "cp-missing"])
        .output()
        .expect("cli dump of missing checkpoint runs");
    assert!(!out.status.success(), "missing checkpoint exits non-zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not found"),
        "error mentions the missing id: {stderr}"
    );
}

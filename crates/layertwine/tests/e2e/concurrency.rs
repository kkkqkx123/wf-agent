//! Concurrency and thread-safety tests.
//!
//! Real scenario: Multiple agents/developers operating on the same repository concurrently.
//! These tests verify that the layered state machine and checkpoint repo handle concurrent
//! read/write operations without corruption.

use crate::common::fixture::{TestConfig, TestEnvironment};
use crate::common::helpers::*;
use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::snapshot::Snapshot;
use layertwine::core::types::SourceType;
use layertwine::engine::diff::diff_to_line_diff;
use layertwine::storage::repository::{DeltaStore, FileNodeStore, SnapshotStore};
use layertwine::storage::SqliteStorage;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

fn make_storage() -> Arc<SqliteStorage> {
    Arc::new(SqliteStorage::new_full_in_memory().unwrap())
}

fn store_snapshot_with_content(
    storage: &SqliteStorage,
    path: &str,
    content: &[u8],
) -> layertwine::core::types::SnapshotId {
    let file = FileNode::new(PathBuf::from(path), content);
    storage.store_file_node(&file, content).unwrap();

    let diff = diff_to_line_diff("", &String::from_utf8_lossy(content));
    let delta = Delta::new(file.clone(), diff, SourceType::Manual);
    storage.store_delta(&delta).unwrap();

    let snapshot = Snapshot::new_initial(file, delta.id);
    storage.store_snapshot(&snapshot, content).unwrap();
    snapshot.id
}

// ---------------------------------------------------------------------------
// Concurrent reads: multiple threads reading from the same storage
// ---------------------------------------------------------------------------

#[test]
fn test_concurrent_reads() {
    let storage = make_storage();
    let sid = store_snapshot_with_content(&storage, "shared.txt", b"hello world");

    let storage_clone = storage.clone();
    let sid_clone = sid;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let s = storage_clone.clone();
            let sid = sid_clone;
            thread::spawn(move || {
                let snapshot = s.get_snapshot(&sid).unwrap();
                assert_eq!(snapshot.file.path_str(), "shared.txt");
                format!("reader-{}: ok", i)
            })
        })
        .collect();

    for h in handles {
        let result = h.join().expect("reader thread panicked");
        assert!(result.contains("ok"));
    }
}

// ---------------------------------------------------------------------------
// Concurrent writes: multiple threads storing independent snapshots
// ---------------------------------------------------------------------------

#[test]
fn test_concurrent_writes() {
    let storage = make_storage();

    let handles: Vec<_> = (0..20)
        .map(|i| {
            let s = storage.clone();
            thread::spawn(move || {
                let content = format!("content-{}", i);
                let sid = store_snapshot_with_content(
                    &s,
                    &format!("f-{}.txt", i),
                    content.as_bytes(),
                );
                // Re-read to verify
                let snapshot = s.get_snapshot(&sid).unwrap();
                assert_eq!(snapshot.file.path_str(), format!("f-{}.txt", i));
                sid
            })
        })
        .collect();

    for h in handles {
        let sid = h.join().expect("writer thread panicked");
        // Verify each snapshot is retrievable
        let snapshot = storage.get_snapshot(&sid).unwrap();
        assert!(!snapshot.deltas.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Concurrent mixed read/write: overlapping storage operations
// ---------------------------------------------------------------------------

#[test]
fn test_concurrent_mixed_read_write() {
    let storage = make_storage();
    let barrier = Arc::new(std::sync::Barrier::new(5));

    // 2 writers (return bool from snapshot_exists)
    let writer_handles: Vec<_> = (0..2)
        .map(|i| {
            let s = storage.clone();
            let b = barrier.clone();
            thread::spawn(move || {
                b.wait();
                let sid =
                    store_snapshot_with_content(&s, &format!("w-{}.txt", i), b"writer data");
                s.snapshot_exists(&sid).unwrap()
            })
        })
        .collect();

    // 3 readers (return usize from count)
    let reader_handles: Vec<_> = (0..3)
        .map(|i| {
            let s = storage.clone();
            let b = barrier.clone();
            thread::spawn(move || {
                b.wait();
                let snapshots = s
                    .find_snapshots_by_file(&format!("w-{}.txt", i % 2))
                    .unwrap_or_default();
                snapshots.len()
            })
        })
        .collect();

    for h in writer_handles {
        let _ = h.join().expect("writer thread panicked");
    }
    for h in reader_handles {
        let _ = h.join().expect("reader thread panicked");
    }
}

// ---------------------------------------------------------------------------
// Concurrent agent operations through ApiService
// ---------------------------------------------------------------------------

#[test]
fn test_parallel_agent_edits() {
    let config = TestConfig::default();
    let env = TestEnvironment::new(config);
    init_repository(&env);
    apply_edit(&env, "base.rs", "// shared base\nfn main() {}\n");
    commit_changes(&env, "base", "dev");

    // Simulate 3 agents editing concurrently via separate threads
    let env_arc = Arc::new(env);
    let mut handles = vec![];

    for agent_id in &["agent-a", "agent-b", "agent-c"] {
        let e = env_arc.clone();
        let aid = agent_id.to_string();
        handles.push(thread::spawn(move || {
            let sid = apply_agent_edit(
                &e,
                &aid,
                "base.rs",
                &format!("// edit by {}\nfn main() {{}}\n", aid),
            );
            sid
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.len(), 3);

    // Verify all agent partitions were created (1 initial + 3 new = 4)
    let agent_parts = get_partitions_by_layer(
        &env_arc,
        layertwine::core::types::LayerType::AgentEdit,
    );
    assert!(
        agent_parts.len() >= 3,
        "should have at least 3 agent partitions, got {}",
        agent_parts.len()
    );
}
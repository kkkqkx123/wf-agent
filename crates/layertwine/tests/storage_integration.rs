//! Integration tests for the storage module.
//!
//! These tests exercise the SQLite-based storage layer end-to-end.
//! They verify:
//! - Snapshot storage and retrieval
//! - Delta storage and retrieval
//! - Partition creation and management
//! - File node storage and content retrieval
//! - Checkpoint, Branch, and DAG operations
//! - Layer management
//! - Atomic transaction guarantees
//! - Database maintenance operations

use std::path::PathBuf;

use layertwine::core::delta::Delta;
use layertwine::core::file_node::FileNode;
use layertwine::core::partition::Partition;
use layertwine::core::snapshot::{Snapshot, SnapshotCompression};
use layertwine::core::types::{
    AgentInstanceId, ContentId, DiffOp, Hunk, LayerType, LineDiff, PartitionType, SourceType,
};
use layertwine::storage::repository::{
    AtomicOps, CheckpointPersist, DeltaStore, FileNodeStore, LayerStore, MetadataStore,
    PartitionStore, SnapshotStore,
};
use layertwine::storage::SqliteStorage;
use layertwine::StorageResult;

fn create_test_storage() -> StorageResult<SqliteStorage> {
    SqliteStorage::new_in_memory()
}

// ---------------------------------------------------------------------------
// SnapshotStore Tests
// ---------------------------------------------------------------------------

#[test]
fn test_store_and_get_snapshot() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let file_path = PathBuf::from("test.txt");
    let file_hash = ContentId::from_content(b"hello world").0;
    let snapshot_id = ContentId::from_content(b"snapshot1");

    let snapshot = Snapshot {
        id: snapshot_id,
        file: FileNode {
            file_path: file_path.clone(),
            base_hash: file_hash,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Manual.name(),
        created_at: chrono::Utc::now().timestamp_millis(),
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    storage.store_snapshot(&snapshot, b"hello world")?;
    let retrieved = storage.get_snapshot(&snapshot_id)?;

    assert_eq!(retrieved.id, snapshot_id);
    assert_eq!(retrieved.file.path_str(), "test.txt");
    assert_eq!(retrieved.file.base_hash, file_hash);
    assert_eq!(retrieved.partition_type, PartitionType::Manual.name());
    assert!(!retrieved.has_conflicts);

    Ok(())
}

#[test]
fn test_snapshot_exists() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");
    let snapshot = Snapshot {
        id: snapshot_id,
        file: FileNode {
            file_path: PathBuf::from("test.txt"),
            base_hash: ContentId::from_content(b"content").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Manual.name(),
        created_at: chrono::Utc::now().timestamp_millis(),
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    assert!(!storage.snapshot_exists(&snapshot_id)?);
    storage.store_snapshot(&snapshot, b"content")?;
    assert!(storage.snapshot_exists(&snapshot_id)?);

    Ok(())
}

#[test]
fn test_find_snapshots_by_file() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let file_path = "test.txt";
    let agent_id = AgentInstanceId("test-agent".to_string());
    let snapshot1 = Snapshot {
        id: ContentId::from_content(b"snapshot1"),
        file: FileNode {
            file_path: PathBuf::from(file_path),
            base_hash: ContentId::from_content(b"version1").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Manual.name(),
        created_at: 1000,
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    let snapshot2 = Snapshot {
        id: ContentId::from_content(b"snapshot2"),
        file: FileNode {
            file_path: PathBuf::from(file_path),
            base_hash: ContentId::from_content(b"version2").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Agent(agent_id).name(),
        created_at: 2000,
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    storage.store_snapshot(&snapshot1, b"version1")?;
    storage.store_snapshot(&snapshot2, b"version2")?;

    let snapshots = storage.find_snapshots_by_file(file_path)?;
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].created_at, 2000); // Should be ordered by created_at DESC
    assert_eq!(snapshots[1].created_at, 1000);

    Ok(())
}

#[test]
fn test_find_snapshots_by_partition() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let agent_id = AgentInstanceId("test-agent".to_string());
    let snapshot1 = Snapshot {
        id: ContentId::from_content(b"snapshot1"),
        file: FileNode {
            file_path: PathBuf::from("test1.txt"),
            base_hash: ContentId::from_content(b"version1").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Manual.name(),
        created_at: 1000,
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    let snapshot2 = Snapshot {
        id: ContentId::from_content(b"snapshot2"),
        file: FileNode {
            file_path: PathBuf::from("test2.txt"),
            base_hash: ContentId::from_content(b"version2").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Agent(agent_id.clone()).name(),
        created_at: 2000,
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    storage.store_snapshot(&snapshot1, b"version1")?;
    storage.store_snapshot(&snapshot2, b"version2")?;

    let manual_snapshots = storage.find_snapshots_by_partition(&PartitionType::Manual)?;
    assert_eq!(manual_snapshots.len(), 1);
    assert_eq!(manual_snapshots[0].file.path_str(), "test1.txt");

    let agent_snapshots = storage.find_snapshots_by_partition(&PartitionType::Agent(agent_id))?;
    assert_eq!(agent_snapshots.len(), 1);
    assert_eq!(agent_snapshots[0].file.path_str(), "test2.txt");

    Ok(())
}

#[test]
fn test_store_snapshots_batch_atomic() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot1 = Snapshot {
        id: ContentId::from_content(b"batch1"),
        file: FileNode {
            file_path: PathBuf::from("file1.txt"),
            base_hash: ContentId::from_content(b"content1").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Manual.name(),
        created_at: 1000,
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    let snapshot2 = Snapshot {
        id: ContentId::from_content(b"batch2"),
        file: FileNode {
            file_path: PathBuf::from("file2.txt"),
            base_hash: ContentId::from_content(b"content2").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Agent(AgentInstanceId("test-agent".into())).name(),
        created_at: 2000,
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    let snapshot3 = Snapshot {
        id: ContentId::from_content(b"batch3"),
        file: FileNode {
            file_path: PathBuf::from("file3.txt"),
            base_hash: ContentId::from_content(b"content3").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Manual.name(),
        created_at: 3000,
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    // Store snapshots in a batch with atomic guarantee
    storage.store_snapshots_batch(&[
        (&snapshot1, b"content1"),
        (&snapshot2, b"content2"),
        (&snapshot3, b"content3"),
    ])?;

    // Verify all snapshots were stored
    let retrieved1 = storage.get_snapshot(&snapshot1.id)?;
    assert_eq!(retrieved1.file.path_str(), "file1.txt");

    let retrieved2 = storage.get_snapshot(&snapshot2.id)?;
    assert_eq!(retrieved2.file.path_str(), "file2.txt");

    let retrieved3 = storage.get_snapshot(&snapshot3.id)?;
    assert_eq!(retrieved3.file.path_str(), "file3.txt");

    // Verify query by partition
    let manual_snapshots = storage.find_snapshots_by_partition(&PartitionType::Manual)?;
    assert_eq!(manual_snapshots.len(), 2);

    Ok(())
}

#[test]
fn test_store_snapshots_batch_empty() -> StorageResult<()> {
    let storage = create_test_storage()?;

    // Storing empty batch should succeed
    storage.store_snapshots_batch(&[])?;

    Ok(())
}

#[test]
fn test_snapshot_with_deltas_and_parents() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let delta_id = ContentId::from_content(b"delta1");
    let parent_id = ContentId::from_content(b"parent");

    let snapshot = Snapshot {
        id: ContentId::from_content(b"snapshot1"),
        file: FileNode {
            file_path: PathBuf::from("test.txt"),
            base_hash: ContentId::from_content(b"content").0,
        },
        deltas: vec![delta_id],
        parents: vec![parent_id],
        partition_type: PartitionType::Manual.name(),
        created_at: chrono::Utc::now().timestamp_millis(),
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    storage.store_snapshot(&snapshot, b"content")?;
    let retrieved = storage.get_snapshot(&snapshot.id)?;

    assert_eq!(retrieved.deltas.len(), 1);
    assert_eq!(retrieved.deltas[0], delta_id);
    assert_eq!(retrieved.parents.len(), 1);
    assert_eq!(retrieved.parents[0], parent_id);

    Ok(())
}

// ---------------------------------------------------------------------------
// DeltaStore Tests
// ---------------------------------------------------------------------------

#[test]
fn test_store_and_get_delta() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let delta_id = ContentId::from_content(b"delta1");
    let file_hash = ContentId::from_content(b"hello world").0;

    let delta = Delta {
        id: delta_id,
        file: FileNode {
            file_path: PathBuf::from("test.txt"),
            base_hash: file_hash,
        },
        diff: LineDiff {
            hunks: vec![Hunk {
                old_start: 1,
                old_len: 1,
                new_start: 1,
                new_len: 1,
                ops: vec![DiffOp::Replace {
                    old_start: 1,
                    old_count: 1,
                    new_start: 1,
                    lines: vec!["hi".to_string()],
                }],
            }],
        },
        source: SourceType::Manual,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    storage.store_delta(&delta)?;
    let retrieved = storage.get_delta(&delta_id)?;

    assert_eq!(retrieved.id, delta_id);
    assert_eq!(retrieved.file.path_str(), "test.txt");
    assert_eq!(retrieved.diff.hunks.len(), 1);
    matches!(retrieved.source, SourceType::Manual);

    Ok(())
}

#[test]
fn test_delta_exists() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let delta_id = ContentId::from_content(b"delta1");
    let delta = Delta {
        id: delta_id,
        file: FileNode {
            file_path: PathBuf::from("test.txt"),
            base_hash: ContentId::from_content(b"content").0,
        },
        diff: LineDiff { hunks: vec![] },
        source: SourceType::Manual,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    assert!(!storage.delta_exists(&delta_id)?);
    storage.store_delta(&delta)?;
    assert!(storage.delta_exists(&delta_id)?);

    Ok(())
}

#[test]
fn test_get_deltas_batch() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let delta1_id = ContentId::from_content(b"delta1");
    let delta2_id = ContentId::from_content(b"delta2");

    let delta1 = Delta {
        id: delta1_id,
        file: FileNode {
            file_path: PathBuf::from("test1.txt"),
            base_hash: ContentId::from_content(b"content1").0,
        },
        diff: LineDiff { hunks: vec![] },
        source: SourceType::Manual,
        timestamp: 1000,
    };

    let delta2 = Delta {
        id: delta2_id,
        file: FileNode {
            file_path: PathBuf::from("test2.txt"),
            base_hash: ContentId::from_content(b"content2").0,
        },
        diff: LineDiff { hunks: vec![] },
        source: SourceType::Manual,
        timestamp: 2000,
    };

    storage.store_delta(&delta1)?;
    storage.store_delta(&delta2)?;

    let deltas = storage.get_deltas(&[delta1_id, delta2_id])?;
    assert_eq!(deltas.len(), 2);

    let retrieved_ids: Vec<ContentId> = deltas.iter().map(|d| d.id).collect();
    assert!(retrieved_ids.contains(&delta1_id));
    assert!(retrieved_ids.contains(&delta2_id));

    Ok(())
}

#[test]
fn test_get_empty_deltas_batch() -> StorageResult<()> {
    let storage = create_test_storage()?;
    let deltas = storage.get_deltas(&[])?;
    assert!(deltas.is_empty());
    Ok(())
}

#[test]
fn test_get_single_delta_batch() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let delta_id = ContentId::from_content(b"delta1");
    let delta = Delta {
        id: delta_id,
        file: FileNode {
            file_path: PathBuf::from("test.txt"),
            base_hash: ContentId::from_content(b"content").0,
        },
        diff: LineDiff { hunks: vec![] },
        source: SourceType::Manual,
        timestamp: 1000,
    };

    storage.store_delta(&delta)?;
    let deltas = storage.get_deltas(&[delta_id])?;

    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].id, delta_id);

    Ok(())
}

#[test]
fn test_delta_with_agent_source() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let delta_id = ContentId::from_content(b"delta1");
    let agent_instance_id = AgentInstanceId("agent-123".to_string());

    let delta = Delta {
        id: delta_id,
        file: FileNode {
            file_path: PathBuf::from("test.txt"),
            base_hash: ContentId::from_content(b"content").0,
        },
        diff: LineDiff { hunks: vec![] },
        source: SourceType::Agent(agent_instance_id.clone()),
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    storage.store_delta(&delta)?;
    let retrieved = storage.get_delta(&delta_id)?;

    match retrieved.source {
        SourceType::Agent(id) => assert_eq!(id, agent_instance_id),
        _ => panic!("Expected Agent source"),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// PartitionStore Tests
// ---------------------------------------------------------------------------

#[test]
fn test_create_and_get_partition() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");
    let partition_id = uuid::Uuid::now_v7();

    let partition = Partition {
        id: partition_id,
        name: "test_partition".to_string(),
        current_snapshot: snapshot_id,
        history: vec![snapshot_id],
        partition_type: PartitionType::Manual,
    };

    storage.create_partition(&partition)?;
    let retrieved = storage.get_partition(&partition_id)?;

    assert_eq!(retrieved.id, partition_id);
    assert_eq!(retrieved.name, "test_partition");
    assert_eq!(retrieved.current_snapshot, snapshot_id);
    assert_eq!(retrieved.history.len(), 1);
    assert_eq!(retrieved.history[0], snapshot_id);
    assert_eq!(retrieved.partition_type, PartitionType::Manual);

    Ok(())
}

#[test]
fn test_get_partition_by_name() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");
    let partition_id = uuid::Uuid::now_v7();

    let partition = Partition {
        id: partition_id,
        name: "named_partition".to_string(),
        current_snapshot: snapshot_id,
        history: vec![snapshot_id],
        partition_type: PartitionType::Manual,
    };

    storage.create_partition(&partition)?;
    let retrieved = storage.get_partition_by_name("named_partition")?;

    assert_eq!(retrieved.id, partition_id);
    assert_eq!(retrieved.name, "named_partition");

    Ok(())
}

#[test]
fn test_update_partition_pointer() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot1_id = ContentId::from_content(b"snapshot1");
    let snapshot2_id = ContentId::from_content(b"snapshot2");
    let partition_id = uuid::Uuid::now_v7();

    let partition = Partition {
        id: partition_id,
        name: "test_partition".to_string(),
        current_snapshot: snapshot1_id,
        history: vec![snapshot1_id],
        partition_type: PartitionType::Manual,
    };

    storage.create_partition(&partition)?;
    storage.update_pointer(&partition_id, &snapshot2_id)?;

    let retrieved = storage.get_partition(&partition_id)?;
    assert_eq!(retrieved.current_snapshot, snapshot2_id);
    assert_eq!(retrieved.history.len(), 2);
    assert_eq!(retrieved.history[0], snapshot1_id);
    assert_eq!(retrieved.history[1], snapshot2_id);

    Ok(())
}

#[test]
fn test_list_partitions() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");
    let agent_id = AgentInstanceId("test-agent".to_string());

    let partition1 = Partition {
        id: uuid::Uuid::now_v7(),
        name: "partition_a".to_string(),
        current_snapshot: snapshot_id,
        history: vec![snapshot_id],
        partition_type: PartitionType::Manual,
    };

    let partition2 = Partition {
        id: uuid::Uuid::now_v7(),
        name: "partition_b".to_string(),
        current_snapshot: snapshot_id,
        history: vec![snapshot_id],
        partition_type: PartitionType::Agent(agent_id),
    };

    storage.create_partition(&partition1)?;
    storage.create_partition(&partition2)?;

    let partitions = storage.list_partitions()?;
    assert_eq!(partitions.len(), 2);

    let names: Vec<String> = partitions.iter().map(|p| p.name.clone()).collect();
    assert!(names.contains(&"partition_a".to_string()));
    assert!(names.contains(&"partition_b".to_string()));

    Ok(())
}

#[test]
fn test_partition_with_empty_history() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");
    let partition_id = uuid::Uuid::now_v7();

    let partition = Partition {
        id: partition_id,
        name: "test_partition".to_string(),
        current_snapshot: snapshot_id,
        history: vec![],
        partition_type: PartitionType::Manual,
    };

    storage.create_partition(&partition)?;
    let retrieved = storage.get_partition(&partition_id)?;

    assert_eq!(retrieved.history.len(), 0);

    Ok(())
}

// ---------------------------------------------------------------------------
// FileNodeStore Tests
// ---------------------------------------------------------------------------

#[test]
fn test_store_and_get_file_content() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let file_path = "test.txt";
    let file_hash = ContentId::from_content(b"hello world").0;
    let content = b"hello world".to_vec();

    let file_node = FileNode {
        file_path: PathBuf::from(file_path),
        base_hash: file_hash,
    };

    storage.store_file_node(&file_node, &content)?;
    let retrieved = storage.get_file_content(file_path, &file_hash)?;

    assert_eq!(retrieved, content);

    Ok(())
}

#[test]
fn test_file_node_exists() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let file_path = "test.txt";
    let file_hash = ContentId::from_content(b"content").0;
    let content = b"content".to_vec();

    let file_node = FileNode {
        file_path: PathBuf::from(file_path),
        base_hash: file_hash,
    };

    assert!(!storage.file_node_exists(file_path, &file_hash)?);
    storage.store_file_node(&file_node, &content)?;
    assert!(storage.file_node_exists(file_path, &file_hash)?);

    Ok(())
}

#[test]
fn test_file_node_different_hashes_same_path() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let file_path = "test.txt";
    let hash1 = ContentId::from_content(b"version1").0;
    let hash2 = ContentId::from_content(b"version2").0;

    let file_node1 = FileNode {
        file_path: PathBuf::from(file_path),
        base_hash: hash1,
    };

    let file_node2 = FileNode {
        file_path: PathBuf::from(file_path),
        base_hash: hash2,
    };

    storage.store_file_node(&file_node1, b"version1")?;
    storage.store_file_node(&file_node2, b"version2")?;

    assert!(storage.file_node_exists(file_path, &hash1)?);
    assert!(storage.file_node_exists(file_path, &hash2)?);

    let content1 = storage.get_file_content(file_path, &hash1)?;
    let content2 = storage.get_file_content(file_path, &hash2)?;

    assert_eq!(content1, b"version1");
    assert_eq!(content2, b"version2");

    Ok(())
}

// ---------------------------------------------------------------------------
// AtomicOps Tests
// ---------------------------------------------------------------------------

#[test]
fn test_atomic_ops_success() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");

    storage.with_atomic(|storage| {
        let snapshot = Snapshot {
            id: snapshot_id,
            file: FileNode {
                file_path: PathBuf::from("test.txt"),
                base_hash: ContentId::from_content(b"content").0,
            },
            deltas: vec![],
            parents: vec![],
            partition_type: PartitionType::Manual.name(),
            created_at: chrono::Utc::now().timestamp_millis(),
            has_conflicts: false,
            content: None,
            source: String::new(),
            compression: SnapshotCompression::None,
        };

        storage.store_snapshot(&snapshot, b"content")?;
        assert!(storage.snapshot_exists(&snapshot_id)?);
        Ok(())
    })?;

    // Verify the snapshot still exists after atomic operation
    assert!(storage.snapshot_exists(&snapshot_id)?);

    Ok(())
}

#[test]
fn test_atomic_ops_rollback_on_error() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");

    let result = storage.with_atomic::<_, StorageResult<()>>(|storage| {
        let snapshot = Snapshot {
            id: snapshot_id,
            file: FileNode {
                file_path: PathBuf::from("test.txt"),
                base_hash: ContentId::from_content(b"content").0,
            },
            deltas: vec![],
            parents: vec![],
            partition_type: PartitionType::Manual.name(),
            created_at: chrono::Utc::now().timestamp_millis(),
            has_conflicts: false,
            content: None,
            source: String::new(),
            compression: SnapshotCompression::None,
        };

        storage.store_snapshot(&snapshot, b"content")?;

        // Simulate an error
        Err(layertwine::StorageError::NotFound("test error".to_string()))
    });

    // Should return the error
    assert!(result.is_err());

    // Verify the snapshot does NOT exist after rollback
    assert!(!storage.snapshot_exists(&snapshot_id)?);

    Ok(())
}

// ---------------------------------------------------------------------------
// CheckpointStore Tests
// ---------------------------------------------------------------------------

// NOTE: These tests are disabled because the current implementation of
// with_atomic has a deadlock issue. The problem is that with_atomic holds
// the lock, then calls f(self) which tries to acquire the same lock again.
// This needs to be fixed by either:
// 1. Using a reentrant mutex (parking_lot::ReentrantMutex)
// 2. Providing internal methods that accept &Connection
// 3. Changing with_atomic to pass a special atomic context

// #[test]
// fn test_atomic_ops_success() -> StorageResult<()> {
//     let storage = create_test_storage()?;
//
//     let snapshot_id = ContentId::from_content(b"snapshot1");
//
//     storage.with_atomic(|storage| {
//         let snapshot = Snapshot {
//             id: snapshot_id.clone(),
//             file: FileNode {
//                 file_path: PathBuf::from("test.txt"),
//                 base_hash: ContentId::from_content(b"content").0,
//             },
//             deltas: vec![],
//             parents: vec![],
//             partition_type: PartitionType::Manual.name(),
//             created_at: chrono::Utc::now().timestamp_millis(),
//             has_conflicts: false,
// content: None,
// source: String::new(),
// compression: SnapshotCompression::None,
//         };
//
//         storage.store_snapshot(&snapshot, b"content")?;
//         assert!(storage.snapshot_exists(&snapshot_id)?);
//         Ok(())
//     })?;
//
//     // Verify the snapshot still exists after atomic operation
//     assert!(storage.snapshot_exists(&snapshot_id)?);
//
//     Ok(())
// }

// #[test]
// fn test_atomic_ops_rollback_on_error() -> StorageResult<()> {
//     let storage = create_test_storage()?;
//
//     let snapshot_id = ContentId::from_content(b"snapshot1");
//
//     let result = storage.with_atomic::<_, StorageResult<()>>(|storage| {
//         let snapshot = Snapshot {
//             id: snapshot_id.clone(),
//             file: FileNode {
//                 file_path: PathBuf::from("test.txt"),
//                 base_hash: ContentId::from_content(b"content").0,
//             },
//             deltas: vec![],
//             parents: vec![],
//             partition_type: PartitionType::Manual.name(),
//             created_at: chrono::Utc::now().timestamp_millis(),
//             has_conflicts: false,
// content: None,
// source: String::new(),
// compression: SnapshotCompression::None,
//         };
//
//         storage.store_snapshot(&snapshot, b"content")?;
//
//         // Simulate an error
//         return Err(layertwine::StorageError::NotFound("test error".to_string()));
//     });
//
//     // Should return the error
//     assert!(result.is_err());
//
//     // Verify the snapshot was rolled back
//     assert!(!storage.snapshot_exists(&snapshot_id)?);
//
//     Ok(())
// }

// ---------------------------------------------------------------------------
// CheckpointStore Tests
// ---------------------------------------------------------------------------

#[test]
fn test_store_and_get_checkpoint() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    let checkpoint_id = ContentId::from_content(b"checkpoint1");
    let parent_id = ContentId::from_content(b"parent");

    let checkpoint = layertwine::checkpoint::Checkpoint {
        id: checkpoint_id,
        parents: vec![parent_id],
        baseline_snapshots: vec![ContentId::from_content(b"snapshot1")],
        metadata: layertwine::checkpoint::CheckpointMetadata {
            author: "test_user".to_string(),
            message: "test commit".to_string(),
            git_anchor: Some("abc123".to_string()),
        },
        created_at: chrono::Utc::now().timestamp_millis(),
        snapshot_sources: std::collections::HashMap::new(),
    };

    storage.store_checkpoint(&checkpoint)?;
    let retrieved = storage.get_checkpoint(&checkpoint_id)?;

    assert_eq!(retrieved.id, checkpoint_id);
    assert_eq!(retrieved.parents.len(), 1);
    assert_eq!(retrieved.parents[0], parent_id);
    assert_eq!(retrieved.metadata.author, "test_user");
    assert_eq!(retrieved.metadata.message, "test commit");
    assert_eq!(retrieved.metadata.git_anchor, Some("abc123".to_string()));

    Ok(())
}

#[test]
fn test_checkpoint_exists() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    let checkpoint_id = ContentId::from_content(b"checkpoint1");
    let checkpoint = layertwine::checkpoint::Checkpoint {
        id: checkpoint_id,
        parents: vec![],
        baseline_snapshots: vec![],
        metadata: layertwine::checkpoint::CheckpointMetadata {
            author: "test_user".to_string(),
            message: "test".to_string(),
            git_anchor: None,
        },
        created_at: chrono::Utc::now().timestamp_millis(),
        snapshot_sources: std::collections::HashMap::new(),
    };

    assert!(!storage.checkpoint_exists(&checkpoint_id)?);
    storage.store_checkpoint(&checkpoint)?;
    assert!(storage.checkpoint_exists(&checkpoint_id)?);

    Ok(())
}

#[test]
fn test_list_checkpoints() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    let checkpoint1 = layertwine::checkpoint::Checkpoint {
        id: ContentId::from_content(b"checkpoint1"),
        parents: vec![],
        baseline_snapshots: vec![],
        metadata: layertwine::checkpoint::CheckpointMetadata {
            author: "user1".to_string(),
            message: "first".to_string(),
            git_anchor: None,
        },
        created_at: 1000,
        snapshot_sources: std::collections::HashMap::new(),
    };

    let checkpoint2 = layertwine::checkpoint::Checkpoint {
        id: ContentId::from_content(b"checkpoint2"),
        parents: vec![],
        baseline_snapshots: vec![],
        metadata: layertwine::checkpoint::CheckpointMetadata {
            author: "user2".to_string(),
            message: "second".to_string(),
            git_anchor: None,
        },
        created_at: 2000,
        snapshot_sources: std::collections::HashMap::new(),
    };

    storage.store_checkpoint(&checkpoint1)?;
    storage.store_checkpoint(&checkpoint2)?;

    let checkpoints = storage.list_checkpoints()?;
    assert_eq!(checkpoints.len(), 2);
    // Should be ordered by created_at DESC
    assert_eq!(checkpoints[0].metadata.message, "second");
    assert_eq!(checkpoints[1].metadata.message, "first");

    Ok(())
}

#[test]
fn test_delete_checkpoint() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    let checkpoint_id = ContentId::from_content(b"checkpoint1");
    let checkpoint = layertwine::checkpoint::Checkpoint {
        id: checkpoint_id,
        parents: vec![],
        baseline_snapshots: vec![],
        metadata: layertwine::checkpoint::CheckpointMetadata {
            author: "user".to_string(),
            message: "test".to_string(),
            git_anchor: None,
        },
        created_at: chrono::Utc::now().timestamp_millis(),
        snapshot_sources: std::collections::HashMap::new(),
    };

    storage.store_checkpoint(&checkpoint)?;
    assert!(storage.checkpoint_exists(&checkpoint_id)?);

    storage.delete_checkpoint(&checkpoint_id)?;
    assert!(!storage.checkpoint_exists(&checkpoint_id)?);

    Ok(())
}

// ---------------------------------------------------------------------------
// BranchStore Tests
// ---------------------------------------------------------------------------

#[test]
fn test_store_and_get_branch() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    let branch_id = ContentId::from_content(b"checkpoint1");

    let branch = layertwine::checkpoint::branch::Branch {
        name: "main".to_string(),
        head: branch_id,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
    };

    storage.store_branch(&branch)?;
    let retrieved = storage.get_branch("main")?;

    assert_eq!(retrieved.name, "main");
    assert_eq!(retrieved.head, branch_id);

    Ok(())
}

#[test]
fn test_update_branch_head() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    let old_head = ContentId::from_content(b"checkpoint1");
    let new_head = ContentId::from_content(b"checkpoint2");

    let branch = layertwine::checkpoint::branch::Branch {
        name: "main".to_string(),
        head: old_head,
        created_at: 1000,
        updated_at: 1000,
    };

    storage.store_branch(&branch)?;
    storage.update_branch_head("main", &new_head)?;

    let retrieved = storage.get_branch("main")?;
    assert_eq!(retrieved.head, new_head);
    assert!(retrieved.updated_at > 1000);

    Ok(())
}

#[test]
fn test_list_branches() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    let branch1 = layertwine::checkpoint::branch::Branch {
        name: "main".to_string(),
        head: ContentId::from_content(b"checkpoint1"),
        created_at: 1000,
        updated_at: 1000,
    };

    let branch2 = layertwine::checkpoint::branch::Branch {
        name: "feature".to_string(),
        head: ContentId::from_content(b"checkpoint2"),
        created_at: 2000,
        updated_at: 2000,
    };

    storage.store_branch(&branch1)?;
    storage.store_branch(&branch2)?;

    let branches = storage.list_branches()?;
    assert_eq!(branches.len(), 2);

    let names: Vec<String> = branches.iter().map(|b| b.name.clone()).collect();
    assert!(names.contains(&"main".to_string()));
    assert!(names.contains(&"feature".to_string()));

    Ok(())
}

#[test]
fn test_delete_branch() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    let branch = layertwine::checkpoint::branch::Branch {
        name: "test_branch".to_string(),
        head: ContentId::from_content(b"checkpoint1"),
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
    };

    storage.store_branch(&branch)?;
    storage.delete_branch("test_branch")?;

    let result = storage.get_branch("test_branch");
    assert!(result.is_err());

    Ok(())
}

// ---------------------------------------------------------------------------
// MetadataStore Tests
// ---------------------------------------------------------------------------

#[test]
fn test_store_and_load_metadata() -> StorageResult<()> {
    let storage = SqliteStorage::new_full_in_memory()?;

    storage.store_metadata("key1", "value1")?;
    storage.store_metadata("key2", "value2")?;

    let value1 = storage.load_metadata("key1")?;
    assert_eq!(value1, Some("value1".to_string()));

    let value2 = storage.load_metadata("key2")?;
    assert_eq!(value2, Some("value2".to_string()));

    let value3 = storage.load_metadata("key3")?;
    assert_eq!(value3, None);

    Ok(())
}

// ---------------------------------------------------------------------------
// LayerStore Tests
// ---------------------------------------------------------------------------

#[test]
fn test_store_and_get_layer() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let partition_id1 = uuid::Uuid::now_v7();
    let partition_id2 = uuid::Uuid::now_v7();

    let layer = layertwine::core::layer::Layer {
        layer_type: LayerType::ManualEdit,
        partitions: vec![partition_id1, partition_id2],
    };

    storage.store_layer(&layer)?;
    let retrieved = storage.get_layer(&LayerType::ManualEdit)?;

    assert_eq!(retrieved.layer_type, LayerType::ManualEdit);
    assert_eq!(retrieved.partitions.len(), 2);
    assert!(retrieved.partitions.contains(&partition_id1));
    assert!(retrieved.partitions.contains(&partition_id2));

    Ok(())
}

#[test]
fn test_list_layer_types() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let layer1 = layertwine::core::layer::Layer {
        layer_type: LayerType::ManualEdit,
        partitions: vec![uuid::Uuid::now_v7()],
    };

    let layer2 = layertwine::core::layer::Layer {
        layer_type: LayerType::AgentEdit,
        partitions: vec![uuid::Uuid::now_v7()],
    };

    storage.store_layer(&layer1)?;
    storage.store_layer(&layer2)?;

    let layer_types = storage.list_layer_types()?;
    assert_eq!(layer_types.len(), 2);
    assert!(layer_types.contains(&LayerType::ManualEdit));
    assert!(layer_types.contains(&LayerType::AgentEdit));

    Ok(())
}

#[test]
fn test_delete_layer() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let layer = layertwine::core::layer::Layer {
        layer_type: LayerType::ManualEdit,
        partitions: vec![uuid::Uuid::now_v7()],
    };

    storage.store_layer(&layer)?;
    storage.delete_layer(&LayerType::ManualEdit)?;

    let result = storage.get_layer(&LayerType::ManualEdit);
    assert!(result.is_err());

    Ok(())
}

// ---------------------------------------------------------------------------
// Clone and Share Tests
// ---------------------------------------------------------------------------

#[test]
fn test_clone_storage() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");
    let snapshot = Snapshot {
        id: snapshot_id,
        file: FileNode {
            file_path: PathBuf::from("test.txt"),
            base_hash: ContentId::from_content(b"content").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Manual.name(),
        created_at: chrono::Utc::now().timestamp_millis(),
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    storage.store_snapshot(&snapshot, b"content")?;

    let cloned_storage = storage.clone();
    let retrieved = cloned_storage.get_snapshot(&snapshot_id)?;

    assert_eq!(retrieved.id, snapshot_id);

    Ok(())
}

#[test]
fn test_share_storage() -> StorageResult<()> {
    let storage = create_test_storage()?;

    let snapshot_id = ContentId::from_content(b"snapshot1");
    let snapshot = Snapshot {
        id: snapshot_id,
        file: FileNode {
            file_path: PathBuf::from("test.txt"),
            base_hash: ContentId::from_content(b"content").0,
        },
        deltas: vec![],
        parents: vec![],
        partition_type: PartitionType::Manual.name(),
        created_at: chrono::Utc::now().timestamp_millis(),
        has_conflicts: false,
        content: None,
        source: String::new(),
        compression: SnapshotCompression::None,
    };

    storage.store_snapshot(&snapshot, b"content")?;

    let shared_storage = storage.share();
    let retrieved = shared_storage.get_snapshot(&snapshot_id)?;

    assert_eq!(retrieved.id, snapshot_id);

    Ok(())
}

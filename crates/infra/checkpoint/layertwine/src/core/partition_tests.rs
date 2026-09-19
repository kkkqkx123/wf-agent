use crate::core::partition::Partition;
use crate::core::types::{PartitionType, SnapshotId};

fn create_test_snapshot_id() -> SnapshotId {
    SnapshotId::from_content(b"snapshot content")
}

#[test]
fn test_partition_new() {
    let name = "test_partition".to_string();
    let partition_type = PartitionType::Manual;
    let initial_snapshot = create_test_snapshot_id();

    let partition = Partition::new(name.clone(), partition_type.clone(), initial_snapshot);

    assert_eq!(partition.name, name);
    assert_eq!(partition.partition_type, partition_type);
    assert_eq!(partition.current_snapshot, initial_snapshot);
    assert_eq!(partition.history.len(), 1);
    assert_eq!(partition.history[0], initial_snapshot);
}

#[test]
fn test_partition_advance() {
    let partition_type = PartitionType::Manual;
    let snapshot1 = SnapshotId::from_content(b"snapshot 1");
    let snapshot2 = SnapshotId::from_content(b"snapshot 2");

    let mut partition = Partition::new("test".to_string(), partition_type, snapshot1);

    partition.advance(snapshot2);

    assert_eq!(partition.current_snapshot, snapshot2);
    assert_eq!(partition.history.len(), 2);
    assert_eq!(partition.history[1], snapshot2);
}

#[test]
fn test_partition_advance_multiple() {
    let partition_type = PartitionType::Manual;
    let snapshot1 = SnapshotId::from_content(b"snapshot 1");
    let snapshot2 = SnapshotId::from_content(b"snapshot 2");
    let snapshot3 = SnapshotId::from_content(b"snapshot 3");

    let mut partition = Partition::new("test".to_string(), partition_type, snapshot1);

    partition.advance(snapshot2);
    partition.advance(snapshot3);

    assert_eq!(partition.current_snapshot, snapshot3);
    assert_eq!(partition.history.len(), 3);
    assert_eq!(partition.history[2], snapshot3);
}

#[test]
fn test_partition_rollback_to_valid() {
    let partition_type = PartitionType::Manual;
    let snapshot1 = SnapshotId::from_content(b"snapshot 1");
    let snapshot2 = SnapshotId::from_content(b"snapshot 2");
    let snapshot3 = SnapshotId::from_content(b"snapshot 3");

    let mut partition = Partition::new("test".to_string(), partition_type, snapshot1);
    partition.advance(snapshot2);
    partition.advance(snapshot3);

    let result = partition.rollback_to(&snapshot2);

    assert!(result);
    assert_eq!(partition.current_snapshot, snapshot2);
    assert_eq!(partition.history.len(), 2);
    assert_eq!(partition.history[1], snapshot2);
}

#[test]
fn test_partition_rollback_to_invalid() {
    let partition_type = PartitionType::Manual;
    let snapshot1 = SnapshotId::from_content(b"snapshot 1");
    let snapshot2 = SnapshotId::from_content(b"snapshot 2");
    let nonexistent = SnapshotId::from_content(b"nonexistent");

    let mut partition = Partition::new("test".to_string(), partition_type, snapshot1);
    partition.advance(snapshot2);

    let result = partition.rollback_to(&nonexistent);

    assert!(!result);
    assert_eq!(partition.current_snapshot, snapshot2);
}

#[test]
fn test_partition_rollback_to_initial() {
    let partition_type = PartitionType::Manual;
    let snapshot1 = SnapshotId::from_content(b"snapshot 1");
    let snapshot2 = SnapshotId::from_content(b"snapshot 2");

    let mut partition = Partition::new("test".to_string(), partition_type, snapshot1);
    partition.advance(snapshot2);

    let result = partition.rollback_to(&snapshot1);

    assert!(result);
    assert_eq!(partition.current_snapshot, snapshot1);
    assert_eq!(partition.history.len(), 1);
}

#[test]
fn test_partition_rollback_one_valid() {
    let partition_type = PartitionType::Manual;
    let snapshot1 = SnapshotId::from_content(b"snapshot 1");
    let snapshot2 = SnapshotId::from_content(b"snapshot 2");

    let mut partition = Partition::new("test".to_string(), partition_type, snapshot1);
    partition.advance(snapshot2);

    let result = partition.rollback_one();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), snapshot1);
    assert_eq!(partition.current_snapshot, snapshot1);
    assert_eq!(partition.history.len(), 1);
}

#[test]
fn test_partition_rollback_one_invalid() {
    let partition_type = PartitionType::Manual;
    let snapshot1 = SnapshotId::from_content(b"snapshot 1");

    let mut partition = Partition::new("test".to_string(), partition_type, snapshot1);

    let result = partition.rollback_one();

    assert!(result.is_none());
    assert_eq!(partition.current_snapshot, snapshot1);
    assert_eq!(partition.history.len(), 1);
}

#[test]
fn test_partition_rollback_one_multiple() {
    let partition_type = PartitionType::Manual;
    let snapshot1 = SnapshotId::from_content(b"snapshot 1");
    let snapshot2 = SnapshotId::from_content(b"snapshot 2");
    let snapshot3 = SnapshotId::from_content(b"snapshot 3");

    let mut partition = Partition::new("test".to_string(), partition_type, snapshot1);
    partition.advance(snapshot2);
    partition.advance(snapshot3);

    let result1 = partition.rollback_one();
    assert!(result1.is_some());
    assert_eq!(result1.unwrap(), snapshot2);

    let result2 = partition.rollback_one();
    assert!(result2.is_some());
    assert_eq!(result2.unwrap(), snapshot1);

    let result3 = partition.rollback_one();
    assert!(result3.is_none());
}

#[test]
fn test_partition_with_agent_type() {
    let partition_type = PartitionType::Agent("agent-001".into());
    let snapshot = create_test_snapshot_id();

    let partition = Partition::new("agent_partition".to_string(), partition_type, snapshot);

    match partition.partition_type {
        PartitionType::Agent(id) => assert_eq!(id.to_string(), "agent-001"),
        _ => panic!("Expected Agent partition type"),
    }
}

#[test]
fn test_partition_serialization() {
    let partition_type = PartitionType::Manual;
    let snapshot = create_test_snapshot_id();

    let partition = Partition::new("test".to_string(), partition_type, snapshot);

    let json = serde_json::to_string(&partition).unwrap();
    let partition2: Partition = serde_json::from_str(&json).unwrap();

    assert_eq!(partition.id, partition2.id);
    assert_eq!(partition.name, partition2.name);
    assert_eq!(partition.current_snapshot, partition2.current_snapshot);
}

#[test]
fn test_partition_uuid_uniqueness() {
    let snapshot = create_test_snapshot_id();

    let partition1 = Partition::new("test1".to_string(), PartitionType::Manual, snapshot);
    let partition2 = Partition::new("test2".to_string(), PartitionType::Manual, snapshot);

    assert_ne!(partition1.id, partition2.id);
}

use crate::core::file_node::FileNode;
use crate::core::snapshot::{Snapshot, SnapshotBuilder};
use crate::core::types::{ContentId, DeltaId};

fn create_test_file() -> FileNode {
    FileNode::new("test.txt".into(), b"initial content")
}

fn create_test_delta_id() -> DeltaId {
    DeltaId::from_content(b"delta content")
}

#[test]
fn test_snapshot_new_initial() {
    let file = create_test_file();
    let delta_id = create_test_delta_id();

    let snapshot = Snapshot::new_initial(file.clone(), delta_id);

    assert_ne!(snapshot.id, ContentId([0u8; 32]));
    assert_eq!(snapshot.file, file);
    assert_eq!(snapshot.deltas.len(), 1);
    assert_eq!(snapshot.deltas[0], delta_id);
    assert!(snapshot.parents.is_empty());
    assert_eq!(snapshot.partition_type, "");
}

#[test]
fn test_snapshot_from_parent() {
    let file = create_test_file();
    let delta_id1 = create_test_delta_id();
    let delta_id2 = DeltaId::from_content(b"delta content 2");

    let parent = Snapshot::new_initial(file.clone(), delta_id1);
    let child = Snapshot::from_parent(&parent, delta_id2, "manual".to_string());

    assert_ne!(child.id, ContentId([0u8; 32]));
    assert_eq!(child.deltas.len(), 2);
    assert_eq!(child.deltas[0], delta_id1);
    assert_eq!(child.deltas[1], delta_id2);
    assert_eq!(child.parents.len(), 1);
    assert_eq!(child.parents[0], parent.id);
    assert_eq!(child.partition_type, "manual");
}

#[test]
fn test_snapshot_apply_delta() {
    let file = create_test_file();
    let delta_id1 = create_test_delta_id();
    let delta_id2 = DeltaId::from_content(b"delta content 2");
    let delta_id3 = DeltaId::from_content(b"delta content 3");

    let snapshot1 = Snapshot::new_initial(file.clone(), delta_id1);
    let snapshot2 = snapshot1.apply_delta(delta_id2);
    let snapshot3 = snapshot2.apply_delta(delta_id3);

    assert_eq!(snapshot1.deltas.len(), 1);
    assert_eq!(snapshot2.deltas.len(), 2);
    assert_eq!(snapshot3.deltas.len(), 3);
    assert_eq!(snapshot3.deltas[2], delta_id3);
}

#[test]
fn test_snapshot_merge() {
    let file = create_test_file();
    let delta_id1 = DeltaId::from_content(b"delta 1");
    let delta_id2 = DeltaId::from_content(b"delta 2");

    let parent1 = Snapshot::new_initial(file.clone(), delta_id1);
    let parent2 = Snapshot::new_initial(file.clone(), delta_id2);

    let merge_delta = DeltaId::from_content(b"merge delta");
    let merged = Snapshot::merge(
        vec![&parent1, &parent2],
        merge_delta,
        "merge".to_string(),
        false,
    );

    assert_eq!(merged.deltas.len(), 2);
    assert_eq!(merged.deltas[0], delta_id1);
    assert_eq!(merged.deltas[1], merge_delta);
    assert_eq!(merged.parents.len(), 2);
    assert!(merged.parents.contains(&parent1.id));
    assert!(merged.parents.contains(&parent2.id));
}

#[test]
fn test_snapshot_merge_three_parents() {
    let file = create_test_file();
    let parent1 = Snapshot::new_initial(file.clone(), DeltaId::from_content(b"d1"));
    let parent2 = Snapshot::new_initial(file.clone(), DeltaId::from_content(b"d2"));
    let parent3 = Snapshot::new_initial(file.clone(), DeltaId::from_content(b"d3"));

    let merge_delta = DeltaId::from_content(b"merge delta");
    let merged = Snapshot::merge(
        vec![&parent1, &parent2, &parent3],
        merge_delta,
        "merge".to_string(),
        false,
    );

    assert_eq!(merged.parents.len(), 3);
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn test_snapshot_merge_empty_parents() {
    let merge_delta = DeltaId::from_content(b"merge delta");
    Snapshot::merge(vec![], merge_delta, "merge".to_string(), false);
}

#[test]
fn test_snapshot_compute_id() {
    let file = create_test_file();
    let delta_id = create_test_delta_id();

    let snapshot1 = Snapshot::new_initial(file.clone(), delta_id);
    std::thread::sleep(std::time::Duration::from_millis(10));
    let snapshot2 = Snapshot::new_initial(file, delta_id);

    assert_eq!(snapshot1.id, snapshot2.id);
    assert_ne!(snapshot1.created_at, snapshot2.created_at);
}

#[test]
fn test_snapshot_serialization() {
    let file = create_test_file();
    let delta_id = create_test_delta_id();
    let snapshot = Snapshot::new_initial(file, delta_id);

    let json = serde_json::to_string(&snapshot).unwrap();
    let snapshot2: Snapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(snapshot.id, snapshot2.id);
    assert_eq!(snapshot.deltas, snapshot2.deltas);
}

#[test]
fn test_snapshot_builder_new() {
    let builder = SnapshotBuilder::new();

    let result = builder.build();
    assert!(result.is_err());
}

#[test]
fn test_snapshot_builder_chain() {
    let file = create_test_file();
    let delta_id = create_test_delta_id();
    let parent_id = ContentId::from_content(b"parent");

    let builder = SnapshotBuilder::new()
        .file(file)
        .add_delta(delta_id)
        .with_parent(parent_id)
        .with_partition_type("test".to_string());

    let snapshot = builder.build().unwrap();
    assert_eq!(snapshot.deltas.len(), 1);
    assert_eq!(snapshot.parents.len(), 1);
    assert_eq!(snapshot.partition_type, "test");
}

#[test]
fn test_snapshot_builder_build() {
    let file = create_test_file();
    let delta_id = create_test_delta_id();
    let parent_id = ContentId::from_content(b"parent");

    let builder = SnapshotBuilder::new()
        .file(file)
        .add_delta(delta_id)
        .with_parent(parent_id)
        .with_partition_type("test".to_string());

    let snapshot = builder.build().unwrap();

    assert_ne!(snapshot.id, ContentId([0u8; 32]));
    assert_eq!(snapshot.deltas.len(), 1);
    assert_eq!(snapshot.parents.len(), 1);
    assert_eq!(snapshot.partition_type, "test");
}

#[test]
fn test_snapshot_builder_missing_file() {
    let builder = SnapshotBuilder::new().add_delta(create_test_delta_id());

    let result = builder.build();
    assert!(result.is_err());
}

#[test]
fn test_snapshot_builder_multiple_deltas() {
    let file = create_test_file();
    let delta_id1 = DeltaId::from_content(b"delta 1");
    let delta_id2 = DeltaId::from_content(b"delta 2");

    let snapshot = SnapshotBuilder::new()
        .file(file)
        .add_delta(delta_id1)
        .add_delta(delta_id2)
        .with_partition_type("test".to_string())
        .build()
        .unwrap();

    assert_eq!(snapshot.deltas.len(), 2);
}

#[test]
fn test_snapshot_builder_multiple_parents() {
    let file = create_test_file();
    let parent_id1 = ContentId::from_content(b"parent 1");
    let parent_id2 = ContentId::from_content(b"parent 2");

    let snapshot = SnapshotBuilder::new()
        .file(file)
        .with_parent(parent_id1)
        .with_parent(parent_id2)
        .with_partition_type("merge".to_string())
        .build()
        .unwrap();

    assert_eq!(snapshot.parents.len(), 2);
}

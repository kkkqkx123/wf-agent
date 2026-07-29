use crate::core::layer::Layer;
use crate::core::types::{LayerType, PartitionId};

#[test]
fn test_layer_new() {
    let layer = Layer::new(LayerType::ManualEdit);
    assert_eq!(layer.layer_type, LayerType::ManualEdit);
    assert!(layer.partitions.is_empty());
}

#[test]
fn test_layer_add_partition() {
    let mut layer = Layer::new(LayerType::AgentEdit);
    let id1 = PartitionId::now_v7();
    let id2 = PartitionId::now_v7();

    layer.add_partition(id1);
    assert_eq!(layer.partitions.len(), 1);
    assert!(layer.has_partition(&id1));

    layer.add_partition(id1);
    assert_eq!(layer.partitions.len(), 1);

    layer.add_partition(id2);
    assert_eq!(layer.partitions.len(), 2);
    assert!(layer.has_partition(&id2));
}

#[test]
fn test_layer_remove_partition() {
    let mut layer = Layer::new(LayerType::Approval);
    let id1 = PartitionId::now_v7();
    let id2 = PartitionId::now_v7();

    layer.add_partition(id1);
    layer.add_partition(id2);

    layer.remove_partition(&id1);
    assert!(!layer.has_partition(&id1));
    assert!(layer.has_partition(&id2));
    assert_eq!(layer.partitions.len(), 1);
}

#[test]
fn test_layer_has_partition() {
    let mut layer = Layer::new(LayerType::Staged);
    let id = PartitionId::now_v7();

    assert!(!layer.has_partition(&id));

    layer.add_partition(id);
    assert!(layer.has_partition(&id));
}

#[test]
fn test_layer_serialization() {
    let mut layer = Layer::new(LayerType::ManualEdit);
    let id = PartitionId::now_v7();
    layer.add_partition(id);

    let json = serde_json::to_string(&layer).unwrap();
    let layer2: Layer = serde_json::from_str(&json).unwrap();

    assert_eq!(layer.layer_type, layer2.layer_type);
    assert_eq!(layer.partitions, layer2.partitions);
}

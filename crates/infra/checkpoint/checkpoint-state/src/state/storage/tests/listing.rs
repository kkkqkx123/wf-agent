use super::support::*;
use crate::state::CheckpointStateManager;
use serde_json::json;
use std::sync::Arc;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::CheckpointType;

#[tokio::test]
async fn list_by_entity_filters_correctly() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<TestCheckpoint>::new(storage);

    let cp1 = TestCheckpoint {
        id: "cp-1".to_string(),
        checkpoint_type: None,
        entity_id: "exec-1".to_string(),
        timestamp: 1000,
        data: "x".to_string(),
    };
    let cp2 = TestCheckpoint {
        id: "cp-2".to_string(),
        checkpoint_type: None,
        entity_id: "exec-2".to_string(),
        timestamp: 2000,
        data: "y".to_string(),
    };

    mgr.save(&cp1, "test", "exec-1").await.unwrap();
    mgr.save(&cp2, "test", "exec-2").await.unwrap();

    let list = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "cp-1");
}

#[tokio::test]
async fn list_latest_by_entities_returns_newest_per_entity() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    // Multiple checkpoints per entity, interleaved timestamps.
    for (i, entity) in [(0, "exec-1"), (0, "exec-2"), (1, "exec-1")] {
        mgr.save(
            &make_envelope(
                &format!("cp-{}-{}", entity, i),
                None,
                None,
                1000 + i as i64,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            entity,
        )
        .await
        .unwrap();
    }
    // Unrelated entity must not leak into the IN query.
    mgr.save(
        &make_envelope(
            "cp-other-0",
            None,
            None,
            9000,
            None,
            Some(json!({"state": "x"})),
        ),
        "test",
        "exec-3",
    )
    .await
    .unwrap();

    let latest = mgr
        .list_latest_by_entities(&["exec-1".to_string(), "exec-2".to_string()])
        .await
        .unwrap();

    assert_eq!(latest.len(), 2);
    let by_entity: std::collections::HashMap<_, _> = latest
        .into_iter()
        .map(|m| (m.entity_id.clone(), m.id.clone()))
        .collect();
    assert_eq!(
        by_entity.get("exec-1").map(String::as_str),
        Some("cp-exec-1-1")
    );
    assert_eq!(
        by_entity.get("exec-2").map(String::as_str),
        Some("cp-exec-2-0")
    );
}

#[tokio::test]
async fn list_latest_by_parent_returns_a_child_whose_newest_row_is_a_delta() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);
    let parent = json!({"hierarchy": {"path": "/parent-1/child-/"}});

    mgr.save(
        &make_envelope("child-full", None, None, 1000, None, Some(parent.clone())),
        "workflow_execution",
        "child-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "child-delta",
            Some(CheckpointType::Delta),
            Some("child-full"),
            2000,
            Some(json!({"state": "s1"})),
            None,
        ),
        "workflow_execution",
        "child-1",
    )
    .await
    .unwrap();
    // A second child whose newest row is a full checkpoint.
    mgr.save(
        &make_envelope("other-full", None, None, 1500, None, Some(parent)),
        "workflow_execution",
        "child-2",
    )
    .await
    .unwrap();

    let children = mgr.list_latest_by_parent("parent-1").await.unwrap();
    let by_entity: std::collections::HashMap<_, _> = children
        .into_iter()
        .map(|m| (m.entity_id.clone(), m.id.clone()))
        .collect();
    assert_eq!(by_entity.len(), 2);
    assert_eq!(
        by_entity.get("child-1").map(String::as_str),
        Some("child-delta"),
        "the newest row is a delta, which carries no snapshot and so no parent link"
    );
    assert_eq!(
        by_entity.get("child-2").map(String::as_str),
        Some("other-full")
    );
}

#[tokio::test]
async fn list_latest_by_parent_ignores_checkpoints_of_other_parents() {
    let storage = make_storage();
    let mgr = StorageBackedStateManager::<Envelope>::new(storage);

    mgr.save(
        &make_envelope(
            "mine",
            None,
            None,
            1000,
            None,
            Some(json!({"hierarchy": {"path": "/parent-1/child-1/"}})),
        ),
        "workflow_execution",
        "child-1",
    )
    .await
    .unwrap();
    mgr.save(
        &make_envelope(
            "theirs",
            None,
            None,
            2000,
            None,
            Some(json!({"hierarchy": {"path": "/parent-2/child-2/"}})),
        ),
        "workflow_execution",
        "child-2",
    )
    .await
    .unwrap();

    let children = mgr.list_latest_by_parent("parent-1").await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].entity_id, "child-1");
}

async fn save_five_for_exec1(storage: &Arc<StorageBackend>) -> StorageBackedStateManager<Envelope> {
    let mgr = StorageBackedStateManager::<Envelope>::new(storage.clone());
    for i in 0..5 {
        mgr.save(
            &make_envelope(
                &format!("cp-{i}"),
                None,
                None,
                1000 + i * 100,
                None,
                Some(json!({"state": i})),
            ),
            "test",
            "exec-1",
        )
        .await
        .unwrap();
    }
    mgr
}

#[tokio::test]
async fn list_by_entity_paged_slices_in_timestamp_order() {
    let storage = make_storage();
    let mgr = save_five_for_exec1(&storage).await;

    let first = mgr.list_by_entity_paged("exec-1", 0, 2).await.unwrap();
    assert_eq!(
        first.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["cp-0", "cp-1"]
    );
    let middle = mgr.list_by_entity_paged("exec-1", 2, 2).await.unwrap();
    assert_eq!(
        middle.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["cp-2", "cp-3"]
    );
    let tail = mgr.list_by_entity_paged("exec-1", 4, 2).await.unwrap();
    assert_eq!(
        tail.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["cp-4"]
    );

    // Offset past the end and a zero limit both yield an empty window.
    assert!(mgr
        .list_by_entity_paged("exec-1", 5, 2)
        .await
        .unwrap()
        .is_empty());
    assert!(mgr
        .list_by_entity_paged("exec-1", 0, 0)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn list_by_entity_paged_pages_reassemble_the_full_listing() {
    let storage = make_storage();
    let mgr = save_five_for_exec1(&storage).await;

    let mut paged = Vec::new();
    for offset in [0u64, 2, 4] {
        paged.extend(mgr.list_by_entity_paged("exec-1", offset, 2).await.unwrap());
    }
    let full = mgr.list_by_entity("exec-1").await.unwrap();
    assert_eq!(
        paged.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        full.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn count_by_entity_counts_only_the_entity_and_reflects_deletes() {
    let storage = make_storage();
    let mgr = save_five_for_exec1(&storage).await;

    // A second entity must not be counted.
    mgr.save(
        &make_envelope(
            "cp-other",
            None,
            None,
            9000,
            None,
            Some(json!({"state": "x"})),
        ),
        "test",
        "exec-2",
    )
    .await
    .unwrap();

    assert_eq!(mgr.count_by_entity("exec-1").await.unwrap(), 5);
    assert_eq!(mgr.count_by_entity("exec-2").await.unwrap(), 1);
    assert_eq!(mgr.count_by_entity("exec-3").await.unwrap(), 0);

    mgr.delete("cp-0").await.unwrap();
    assert_eq!(mgr.count_by_entity("exec-1").await.unwrap(), 4);
}

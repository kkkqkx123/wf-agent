use wf_storage::context::{AtomicOperation, EntityStoreId, StorageContext};
use wf_storage::domain::store::{BatchItem, Store, StoreOperation};

fn save_op(id: &str) -> StoreOperation {
    StoreOperation::Save(BatchItem::new(
        id,
        b"payload".to_vec(),
        serde_json::json!({"entityType": "test"}),
    ))
}

async fn assert_atomic_behavior(ctx: &StorageContext) {
    // Multi-target batch lands in every involved store.
    let ops = vec![
        AtomicOperation::new(EntityStoreId::Checkpoint, save_op("cp-1")),
        AtomicOperation::new(EntityStoreId::Task, save_op("task-1")),
        AtomicOperation::new(
            EntityStoreId::Checkpoint,
            StoreOperation::Delete("cp-1".into()),
        ),
    ];
    ctx.apply_atomic(&ops).await.unwrap();
    assert!(!ctx.checkpoint.store().exists("cp-1").await.unwrap());
    assert!(ctx.task.store().exists("task-1").await.unwrap());
    let (data, _) = ctx.task.store().load("task-1").await.unwrap().unwrap();
    assert_eq!(data, b"payload");
    // The multi-target path records one batch observation per involved
    // backend, just like the per-store path does.
    assert_eq!(ctx.checkpoint.store().op_metrics().batch.count(), 1);
    assert_eq!(ctx.task.store().op_metrics().batch.count(), 1);

    // Empty batch is a no-op.
    ctx.apply_atomic(&[]).await.unwrap();

    // Single-target batch uses the normal per-store path.
    ctx.apply_atomic(&[AtomicOperation::new(EntityStoreId::Task, save_op("task-2"))])
        .await
        .unwrap();
    assert!(ctx.task.store().exists("task-2").await.unwrap());
    assert_eq!(ctx.task.store().op_metrics().batch.count(), 2);
    assert_eq!(ctx.checkpoint.store().op_metrics().batch.count(), 1);

    // The cross-table path invalidates the cached rows it touches: a record
    // loaded before the batch must not be served from cache after deletion.
    ctx.apply_atomic(&[AtomicOperation::new(EntityStoreId::Task, save_op("task-cached"))])
        .await
        .unwrap();
    let _ = ctx.task.store().load("task-cached").await.unwrap();
    let cross = vec![
        AtomicOperation::new(
            EntityStoreId::Task,
            StoreOperation::Delete("task-cached".into()),
        ),
        AtomicOperation::new(EntityStoreId::Checkpoint, save_op("cp-cached")),
    ];
    ctx.apply_atomic(&cross).await.unwrap();
    assert!(ctx.task.store().load("task-cached").await.unwrap().is_none());
    assert!(ctx.checkpoint.store().exists("cp-cached").await.unwrap());
}

#[tokio::test]
async fn test_memory_cross_entity_atomic_batch() {
    let ctx = StorageContext::new_memory();
    assert_atomic_behavior(&ctx).await;
}

#[tokio::test]
async fn test_sqlite_cross_entity_atomic_batch() {
    let (ctx, path) = StorageContext::new_test_sqlite().await.unwrap();
    assert_atomic_behavior(&ctx).await;
    drop(ctx);
    std::fs::remove_file(&path).ok();
}

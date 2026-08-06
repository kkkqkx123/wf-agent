use std::collections::HashMap;

use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::checkpoint::{CheckpointListOptions, CheckpointStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::Checkpoint;

use crate::not_found;

pub async fn save_checkpoint(
    ctx: &StorageContext,
    checkpoint: &Checkpoint,
) -> crate::ApiResult<()> {
    ctx.checkpoint.save(checkpoint).await?;
    Ok(())
}

pub async fn get_checkpoint(ctx: &StorageContext, id: &str) -> crate::ApiResult<Checkpoint> {
    ctx.checkpoint
        .load(id)
        .await?
        .ok_or_else(|| not_found("checkpoint", id))
}

pub async fn delete_checkpoint(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.checkpoint.delete(id).await.map_err(Into::into)
}

pub async fn list_checkpoints(
    ctx: &StorageContext,
    options: Option<CheckpointListOptions>,
) -> crate::ApiResult<Vec<Checkpoint>> {
    ctx.checkpoint.list(options).await.map_err(Into::into)
}

pub async fn list_checkpoints_by_entity(
    ctx: &StorageContext,
    entity_id: &str,
    entity_type: &str,
) -> crate::ApiResult<Vec<Checkpoint>> {
    ctx.checkpoint
        .list_by_entity(entity_id, entity_type)
        .await
        .map_err(Into::into)
}

pub async fn get_latest_checkpoint(
    ctx: &StorageContext,
    entity_id: &str,
    entity_type: &str,
) -> crate::ApiResult<Option<Checkpoint>> {
    ctx.checkpoint
        .get_latest_by_entity(entity_id, entity_type)
        .await
        .map_err(Into::into)
}

pub async fn delete_checkpoints_by_entity(
    ctx: &StorageContext,
    entity_id: &str,
    entity_type: &str,
) -> crate::ApiResult<u64> {
    ctx.checkpoint
        .delete_by_entity(entity_id, entity_type)
        .await
        .map_err(Into::into)
}

pub async fn list_checkpoints_by_entities(
    ctx: &StorageContext,
    entity_ids: &[String],
    entity_type: &str,
) -> crate::ApiResult<Vec<Checkpoint>> {
    ctx.checkpoint
        .list_by_entities_with_metadata(entity_ids, entity_type)
        .await
        .map_err(Into::into)
}

pub async fn get_checkpoint_entity_metadata(
    ctx: &StorageContext,
    entity_id: &str,
) -> crate::ApiResult<Option<HashMap<String, Value>>> {
    ctx.checkpoint
        .get_entity_metadata(entity_id)
        .await
        .map_err(Into::into)
}

pub async fn set_checkpoint_entity_metadata(
    ctx: &StorageContext,
    entity_id: &str,
    metadata: &HashMap<String, Value>,
) -> crate::ApiResult<()> {
    ctx.checkpoint
        .set_entity_metadata(entity_id, metadata)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::checkpoint::base::{CheckpointStatus, CheckpointType};

    fn make_checkpoint(id: &str, entity_id: &str, ts: i64) -> Checkpoint {
        Checkpoint {
            id: id.into(),
            entity_type: "execution".into(),
            entity_id: entity_id.into(),
            checkpoint_type: CheckpointType::Full,
            timestamp: ts,
            status: CheckpointStatus::Active,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: None,
            blob_size: None,
            tags: None,
            custom_fields: None,
        }
    }

    #[tokio::test]
    async fn checkpoint_crud() {
        let ctx = StorageContext::new_memory();
        save_checkpoint(&ctx, &make_checkpoint("cp-1", "ex-1", 1000))
            .await
            .unwrap();

        let loaded = get_checkpoint(&ctx, "cp-1").await.unwrap();
        assert_eq!(loaded.entity_id, "ex-1");

        let err = get_checkpoint(&ctx, "cp-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_checkpoint(&ctx, "cp-1").await.unwrap());
        assert!(!delete_checkpoint(&ctx, "cp-1").await.unwrap());
    }

    #[tokio::test]
    async fn checkpoint_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_checkpoint(&ctx, &make_checkpoint("cp-1", "ex-1", 1000))
            .await
            .unwrap();
        save_checkpoint(&ctx, &make_checkpoint("cp-2", "ex-1", 3000))
            .await
            .unwrap();
        save_checkpoint(&ctx, &make_checkpoint("cp-3", "ex-2", 2000))
            .await
            .unwrap();

        // The storage adapter filters checkpoints by their own record type
        // (always "checkpoint"), combined with the checkpointed entity id.
        let by_entity = list_checkpoints_by_entity(&ctx, "ex-1", "checkpoint")
            .await
            .unwrap();
        assert_eq!(by_entity.len(), 2);

        let latest = get_latest_checkpoint(&ctx, "ex-1", "checkpoint")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, "cp-2");

        let multi =
            list_checkpoints_by_entities(&ctx, &["ex-1".into(), "ex-2".into()], "checkpoint")
                .await
                .unwrap();
        assert_eq!(multi.len(), 3);

        let deleted = delete_checkpoints_by_entity(&ctx, "ex-1", "checkpoint")
            .await
            .unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(list_checkpoints(&ctx, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn checkpoint_entity_metadata() {
        let ctx = StorageContext::new_memory();
        save_checkpoint(&ctx, &make_checkpoint("cp-meta", "ex-meta", 1000))
            .await
            .unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("owner".into(), Value::String("alice".into()));
        set_checkpoint_entity_metadata(&ctx, "cp-meta", &metadata)
            .await
            .unwrap();

        let stored = get_checkpoint_entity_metadata(&ctx, "cp-meta")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.get("owner").and_then(|v| v.as_str()), Some("alice"));
    }
}

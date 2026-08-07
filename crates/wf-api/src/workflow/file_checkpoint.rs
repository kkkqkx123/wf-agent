use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::file_checkpoint::{
    FileCheckpointListOptions, FileCheckpointStorageAdapter,
};
use wf_storage::context::StorageContext;
use wf_types::FileCheckpointStorageMetadata;

use crate::not_found;

pub async fn save_file_checkpoint(
    ctx: &StorageContext,
    file_checkpoint: &FileCheckpointStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.file_checkpoint.save(file_checkpoint).await?;
    Ok(())
}

pub async fn get_file_checkpoint(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<FileCheckpointStorageMetadata> {
    ctx.file_checkpoint
        .load(id)
        .await?
        .ok_or_else(|| not_found("file_checkpoint", id))
}

pub async fn delete_file_checkpoint(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.file_checkpoint.delete(id).await.map_err(Into::into)
}

pub async fn list_file_checkpoints(
    ctx: &StorageContext,
    options: Option<FileCheckpointListOptions>,
) -> crate::ApiResult<Vec<FileCheckpointStorageMetadata>> {
    ctx.file_checkpoint.list(options).await.map_err(Into::into)
}

pub async fn load_file_checkpoint_by_path(
    ctx: &StorageContext,
    file_path: &str,
) -> crate::ApiResult<Option<FileCheckpointStorageMetadata>> {
    ctx.file_checkpoint
        .load_by_file_path(file_path)
        .await
        .map_err(Into::into)
}

pub async fn list_file_checkpoints_by_entity(
    ctx: &StorageContext,
    entity_id: &str,
) -> crate::ApiResult<Vec<FileCheckpointStorageMetadata>> {
    ctx.file_checkpoint
        .list_by_entity(entity_id)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file_checkpoint(
        id: &str,
        entity_id: &str,
        path: &str,
    ) -> FileCheckpointStorageMetadata {
        FileCheckpointStorageMetadata {
            id: id.into(),
            entity_id: entity_id.into(),
            file_path: path.into(),
            checkpoint_id: "cp-1".into(),
            size_bytes: 128,
            compressed: false,
            created_at: 1000,
        }
    }

    #[tokio::test]
    async fn file_checkpoint_crud() {
        let ctx = StorageContext::new_memory();
        save_file_checkpoint(&ctx, &make_file_checkpoint("fc-1", "ex-1", "/tmp/a.bin"))
            .await
            .unwrap();

        let loaded = get_file_checkpoint(&ctx, "fc-1").await.unwrap();
        assert_eq!(loaded.file_path, "/tmp/a.bin");

        let err = get_file_checkpoint(&ctx, "fc-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_file_checkpoint(&ctx, "fc-1").await.unwrap());
    }

    #[tokio::test]
    async fn file_checkpoint_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_file_checkpoint(&ctx, &make_file_checkpoint("fc-1", "ex-1", "/tmp/a.bin"))
            .await
            .unwrap();
        save_file_checkpoint(&ctx, &make_file_checkpoint("fc-2", "ex-1", "/tmp/b.bin"))
            .await
            .unwrap();
        save_file_checkpoint(&ctx, &make_file_checkpoint("fc-3", "ex-2", "/tmp/c.bin"))
            .await
            .unwrap();

        let by_path = load_file_checkpoint_by_path(&ctx, "/tmp/b.bin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_path.id, "fc-2");
        assert!(load_file_checkpoint_by_path(&ctx, "/tmp/nope.bin")
            .await
            .unwrap()
            .is_none());

        let by_entity = list_file_checkpoints_by_entity(&ctx, "ex-1").await.unwrap();
        assert_eq!(by_entity.len(), 2);

        let listed = list_file_checkpoints(&ctx, None).await.unwrap();
        assert_eq!(listed.len(), 3);
    }
}

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::task::{TaskListOptions, TaskStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::TaskStorageMetadata;

use crate::not_found;

pub async fn save_task(
    ctx: &StorageContext,
    task: &TaskStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.task.save(task).await?;
    Ok(())
}

pub async fn get_task(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<TaskStorageMetadata> {
    ctx.task
        .load(id)
        .await?
        .ok_or_else(|| not_found("task", id))
}

pub async fn delete_task(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.task.delete(id).await.map_err(Into::into)
}

pub async fn list_tasks(
    ctx: &StorageContext,
    options: Option<TaskListOptions>,
) -> crate::ApiResult<Vec<TaskStorageMetadata>> {
    ctx.task.list(options).await.map_err(Into::into)
}

pub async fn get_task_stats(
    ctx: &StorageContext,
) -> crate::ApiResult<std::collections::HashMap<String, u64>> {
    ctx.task.get_stats().await.map_err(Into::into)
}

pub async fn cleanup_tasks(
    ctx: &StorageContext,
    older_than: i64,
) -> crate::ApiResult<u64> {
    ctx.task.cleanup(older_than).await.map_err(Into::into)
}

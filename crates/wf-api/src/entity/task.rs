use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::task::{TaskListOptions, TaskStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::TaskStorageMetadata;

use crate::not_found;

pub async fn save_task(ctx: &StorageContext, task: &TaskStorageMetadata) -> crate::ApiResult<()> {
    ctx.task.save(task).await?;
    Ok(())
}

pub async fn get_task(ctx: &StorageContext, id: &str) -> crate::ApiResult<TaskStorageMetadata> {
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

pub async fn cleanup_tasks(ctx: &StorageContext, older_than: i64) -> crate::ApiResult<u64> {
    ctx.task.cleanup(older_than).await.map_err(Into::into)
}

/// Cancel a task: flip its status to `cancelled` (read-modify-write through
/// the storage adapter).
pub async fn cancel_task(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    let mut task = get_task(ctx, id).await?;
    if task.status == "cancelled" {
        return Ok(());
    }
    task.status = "cancelled".into();
    task.updated_at = wf_common::now();
    ctx.task.save(&task).await?;
    Ok(())
}

/// Tasks attributed to an execution (matched against the optional
/// `execution_id` field).
pub async fn get_by_execution_id(
    ctx: &StorageContext,
    execution_id: &str,
) -> crate::ApiResult<Vec<TaskStorageMetadata>> {
    Ok(list_tasks(ctx, None)
        .await?
        .into_iter()
        .filter(|t| t.execution_id.as_deref() == Some(execution_id))
        .collect())
}

/// Tasks attributed to an instance (matched against the optional
/// `instance_id` field).
pub async fn get_by_instance_id(
    ctx: &StorageContext,
    instance_id: &str,
) -> crate::ApiResult<Vec<TaskStorageMetadata>> {
    Ok(list_tasks(ctx, None)
        .await?
        .into_iter()
        .filter(|t| t.instance_id.as_deref() == Some(instance_id))
        .collect())
}

/// Remove all tasks from storage; returns the number removed.
pub async fn clear_tasks(ctx: &StorageContext) -> crate::ApiResult<u64> {
    let all = list_tasks(ctx, None).await?;
    let count = all.len() as u64;
    for task in all {
        let _ = delete_task(ctx, &task.id).await;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, task_type: &str, status: &str, created_at: i64) -> TaskStorageMetadata {
        TaskStorageMetadata {
            id: id.into(),
            task_type: task_type.into(),
            status: status.into(),
            execution_id: None,
            instance_id: None,
            created_at,
            updated_at: created_at,
        }
    }

    #[tokio::test]
    async fn task_crud() {
        let ctx = StorageContext::new_memory();
        save_task(&ctx, &make_task("task-1", "ingest", "pending", 1000))
            .await
            .unwrap();

        let loaded = get_task(&ctx, "task-1").await.unwrap();
        assert_eq!(loaded.status, "pending");

        let err = get_task(&ctx, "task-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_task(&ctx, "task-1").await.unwrap());
        assert!(!delete_task(&ctx, "task-1").await.unwrap());
    }

    #[tokio::test]
    async fn task_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_task(&ctx, &make_task("task-1", "ingest", "pending", 1000))
            .await
            .unwrap();
        save_task(&ctx, &make_task("task-2", "ingest", "running", 2000))
            .await
            .unwrap();
        save_task(&ctx, &make_task("task-3", "cleanup", "completed", 3000))
            .await
            .unwrap();

        let all = list_tasks(&ctx, None).await.unwrap();
        assert_eq!(all.len(), 3);

        let filtered = list_tasks(
            &ctx,
            Some(TaskListOptions {
                offset: None,
                limit: None,
                status_filter: Some("running".into()),
                task_type_filter: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(filtered.len(), 1);

        let stats = get_task_stats(&ctx).await.unwrap();
        assert_eq!(stats.get("pending"), Some(&1));
        assert_eq!(stats.get("running"), Some(&1));
        assert_eq!(stats.get("completed"), Some(&1));

        // Cleanup removes tasks older than the cutoff (created_at < cutoff).
        let removed = cleanup_tasks(&ctx, 1500).await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(list_tasks(&ctx, None).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn cancel_and_lookup_by_ids() {
        let ctx = StorageContext::new_memory();
        let mut task = make_task("task-x", "ingest", "pending", 1000);
        task.execution_id = Some("exec-x".into());
        task.instance_id = Some("inst-x".into());
        save_task(&ctx, &task).await.unwrap();

        cancel_task(&ctx, "task-x").await.unwrap();
        assert_eq!(get_task(&ctx, "task-x").await.unwrap().status, "cancelled");

        // Cancelling again is idempotent.
        cancel_task(&ctx, "task-x").await.unwrap();

        let by_exec = get_by_execution_id(&ctx, "exec-x").await.unwrap();
        assert_eq!(by_exec.len(), 1);
        let by_inst = get_by_instance_id(&ctx, "inst-x").await.unwrap();
        assert_eq!(by_inst.len(), 1);
        assert!(get_by_execution_id(&ctx, "nope").await.unwrap().is_empty());

        let err = cancel_task(&ctx, "missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert_eq!(clear_tasks(&ctx).await.unwrap(), 1);
        assert!(list_tasks(&ctx, None).await.unwrap().is_empty());
    }
}

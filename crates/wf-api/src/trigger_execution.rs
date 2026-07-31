use std::collections::HashMap;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::trigger_execution::{
    TriggerExecutionListOptions, TriggerExecutionStorageAdapter,
};
use wf_storage::context::StorageContext;
use wf_types::TriggerExecutionStorageMetadata;

use crate::not_found;

pub async fn save_trigger_execution(
    ctx: &StorageContext,
    execution: &TriggerExecutionStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.trigger_execution.save(execution).await?;
    Ok(())
}

pub async fn get_trigger_execution(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<TriggerExecutionStorageMetadata> {
    ctx.trigger_execution
        .load(id)
        .await?
        .ok_or_else(|| not_found("trigger_execution", id))
}

pub async fn list_trigger_executions(
    ctx: &StorageContext,
    options: Option<TriggerExecutionListOptions>,
) -> crate::ApiResult<Vec<TriggerExecutionStorageMetadata>> {
    ctx.trigger_execution
        .list(options)
        .await
        .map_err(Into::into)
}

pub async fn list_by_trigger_name(
    ctx: &StorageContext,
    trigger_name: &str,
) -> crate::ApiResult<Vec<TriggerExecutionStorageMetadata>> {
    ctx.trigger_execution
        .list_by_trigger(trigger_name)
        .await
        .map_err(Into::into)
}

pub async fn list_by_execution(
    ctx: &StorageContext,
    execution_id: &str,
) -> crate::ApiResult<Vec<TriggerExecutionStorageMetadata>> {
    ctx.trigger_execution
        .list_by_execution(execution_id)
        .await
        .map_err(Into::into)
}

pub async fn list_by_workflow(
    ctx: &StorageContext,
    workflow_id: &str,
) -> crate::ApiResult<Vec<TriggerExecutionStorageMetadata>> {
    ctx.trigger_execution
        .list_by_workflow(workflow_id)
        .await
        .map_err(Into::into)
}

pub async fn get_trigger_execution_stats(
    ctx: &StorageContext,
) -> crate::ApiResult<HashMap<String, u64>> {
    ctx.trigger_execution.get_stats().await.map_err(Into::into)
}

pub async fn cleanup_old_trigger_executions(
    ctx: &StorageContext,
    older_than: i64,
) -> crate::ApiResult<u64> {
    ctx.trigger_execution
        .cleanup(older_than)
        .await
        .map_err(Into::into)
}

pub async fn delete_trigger_execution(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.trigger_execution.delete(id).await.map_err(Into::into)
}

use std::collections::HashMap;

use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::execution::{
    WorkflowExecutionListOptions, WorkflowExecutionStorageAdapter,
};
use wf_storage::adapter::workflow::{WorkflowListOptions, WorkflowStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::{ExecutionStatus, WorkflowDefinition, WorkflowExecution};

use crate::not_found;

pub async fn save_workflow(
    ctx: &StorageContext,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    ctx.workflow.save(workflow).await?;
    Ok(())
}

pub async fn get_workflow(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<WorkflowDefinition> {
    ctx.workflow
        .load(id)
        .await?
        .ok_or_else(|| not_found("workflow", id))
}

pub async fn delete_workflow(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.workflow.delete(id).await.map_err(Into::into)
}

pub async fn list_workflows(
    ctx: &StorageContext,
    options: Option<WorkflowListOptions>,
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    ctx.workflow.list(options).await.map_err(Into::into)
}

pub async fn update_workflow_metadata(
    ctx: &StorageContext,
    id: &str,
    metadata: &HashMap<String, Value>,
) -> crate::ApiResult<()> {
    ctx.workflow.update_metadata(id, metadata).await?;
    Ok(())
}

pub async fn save_workflow_version(
    ctx: &StorageContext,
    workflow_id: &str,
    version: &str,
    template: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    ctx.workflow
        .save_version(workflow_id, version, template)
        .await?;
    Ok(())
}

pub async fn get_workflow_version(
    ctx: &StorageContext,
    workflow_id: &str,
    version: &str,
) -> crate::ApiResult<WorkflowDefinition> {
    ctx.workflow
        .load_version(workflow_id, version)
        .await?
        .ok_or_else(|| not_found("workflow_version", &format!("{}:v{}", workflow_id, version)))
}

pub async fn list_workflow_versions(
    ctx: &StorageContext,
    workflow_id: &str,
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    ctx.workflow
        .list_versions(workflow_id)
        .await
        .map_err(Into::into)
}

pub async fn save_execution(
    ctx: &StorageContext,
    execution: &WorkflowExecution,
) -> crate::ApiResult<()> {
    ctx.workflow_execution.save(execution).await?;
    Ok(())
}

pub async fn get_execution(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<WorkflowExecution> {
    ctx.workflow_execution
        .load(id)
        .await?
        .ok_or_else(|| not_found("execution", id))
}

pub async fn delete_execution(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.workflow_execution.delete(id).await.map_err(Into::into)
}

pub async fn list_executions(
    ctx: &StorageContext,
    options: Option<WorkflowExecutionListOptions>,
) -> crate::ApiResult<Vec<WorkflowExecution>> {
    ctx.workflow_execution
        .list(options)
        .await
        .map_err(Into::into)
}

pub async fn update_execution_status(
    ctx: &StorageContext,
    id: &str,
    status: &ExecutionStatus,
) -> crate::ApiResult<()> {
    ctx.workflow_execution.update_status(id, status).await?;
    Ok(())
}

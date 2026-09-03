//! Workflow execution storage operations.

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::execution::{
    WorkflowExecutionListOptions, WorkflowExecutionStorageAdapter,
};
use wf_types::{ExecutionStatus, WorkflowExecution};

use crate::ApiContext;

pub async fn save_execution(
    ctx: &ApiContext,
    execution: &WorkflowExecution,
) -> crate::ApiResult<()> {
    ctx.storage.workflow_execution.save(execution).await?;
    Ok(())
}

pub async fn get_execution(ctx: &ApiContext, id: &str) -> crate::ApiResult<WorkflowExecution> {
    ctx.storage
        .workflow_execution
        .load(id)
        .await?
        .ok_or_else(|| crate::not_found("execution", id))
}

pub async fn delete_execution(ctx: &ApiContext, id: &str) -> crate::ApiResult<bool> {
    ctx.storage
        .workflow_execution
        .delete(id)
        .await
        .map_err(Into::into)
}

/// Delete an execution and all related records (agent execution, agent loop).
/// Returns true when the primary workflow execution record was deleted.
pub async fn delete_execution_full(ctx: &ApiContext, id: &str) -> crate::ApiResult<bool> {
    let deleted = ctx.storage.workflow_execution.delete(id).await?;
    let _ = ctx.storage.agent_execution.delete(id).await;
    let _ = ctx.storage.agent_loop.delete(id).await;
    Ok(deleted)
}

pub async fn list_executions(
    ctx: &ApiContext,
    options: Option<WorkflowExecutionListOptions>,
) -> crate::ApiResult<Vec<WorkflowExecution>> {
    ctx.storage
        .workflow_execution
        .list(options)
        .await
        .map_err(Into::into)
}

pub async fn update_execution_status(
    ctx: &ApiContext,
    id: &str,
    status: &ExecutionStatus,
) -> crate::ApiResult<()> {
    ctx.storage
        .workflow_execution
        .update_status(id, status)
        .await?;
    Ok(())
}

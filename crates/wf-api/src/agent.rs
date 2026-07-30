use wf_storage::adapter::agent_execution::{
    AgentExecutionListOptions, AgentExecutionStorageAdapter,
};
use wf_storage::adapter::agent_loop::{AgentLoopListOptions, AgentLoopStorageAdapter};
use wf_storage::adapter::agent_profile::AgentProfileListOptions;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::{
    AgentExecution, AgentLoopStorageMetadata, AgentProfileStorageMetadata,
};

use crate::not_found;

pub async fn save_agent_profile(
    ctx: &StorageContext,
    profile: &AgentProfileStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.agent_profile.save(profile).await?;
    Ok(())
}

pub async fn get_agent_profile(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<AgentProfileStorageMetadata> {
    ctx.agent_profile
        .load(id)
        .await?
        .ok_or_else(|| not_found("agent_profile", id))
}

pub async fn delete_agent_profile(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.agent_profile.delete(id).await.map_err(Into::into)
}

pub async fn list_agent_profiles(
    ctx: &StorageContext,
    options: Option<AgentProfileListOptions>,
) -> crate::ApiResult<Vec<AgentProfileStorageMetadata>> {
    ctx.agent_profile.list(options).await.map_err(Into::into)
}

pub async fn save_agent_loop(
    ctx: &StorageContext,
    loop_def: &AgentLoopStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.agent_loop.save(loop_def).await?;
    Ok(())
}

pub async fn get_agent_loop(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<AgentLoopStorageMetadata> {
    ctx.agent_loop
        .load(id)
        .await?
        .ok_or_else(|| not_found("agent_loop", id))
}

pub async fn delete_agent_loop(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.agent_loop.delete(id).await.map_err(Into::into)
}

pub async fn list_agent_loops(
    ctx: &StorageContext,
    options: Option<AgentLoopListOptions>,
) -> crate::ApiResult<Vec<AgentLoopStorageMetadata>> {
    ctx.agent_loop.list(options).await.map_err(Into::into)
}

pub async fn update_agent_loop_status(
    ctx: &StorageContext,
    id: &str,
    status: &str,
) -> crate::ApiResult<()> {
    ctx.agent_loop.update_status(id, status).await?;
    Ok(())
}

pub async fn save_agent_execution(
    ctx: &StorageContext,
    execution: &AgentExecution,
) -> crate::ApiResult<()> {
    ctx.agent_execution.save(execution).await?;
    Ok(())
}

pub async fn get_agent_execution(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<AgentExecution> {
    ctx.agent_execution
        .load(id)
        .await?
        .ok_or_else(|| not_found("agent_execution", id))
}

pub async fn delete_agent_execution(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.agent_execution.delete(id).await.map_err(Into::into)
}

pub async fn list_agent_executions(
    ctx: &StorageContext,
    options: Option<AgentExecutionListOptions>,
) -> crate::ApiResult<Vec<AgentExecution>> {
    ctx.agent_execution.list(options).await.map_err(Into::into)
}

pub async fn list_executions_by_definition(
    ctx: &StorageContext,
    definition_id: &str,
) -> crate::ApiResult<Vec<AgentExecution>> {
    ctx.agent_execution
        .list_by_definition(definition_id)
        .await
        .map_err(Into::into)
}

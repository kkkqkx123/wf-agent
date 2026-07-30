use std::collections::HashMap;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::user_interaction::{
    UserInteractionListOptions, UserInteractionStorageAdapter,
};
use wf_storage::context::StorageContext;
use wf_types::UserInteractionStorageMetadata;

use crate::not_found;

pub async fn save_interaction(
    ctx: &StorageContext,
    interaction: &UserInteractionStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.user_interaction.save(interaction).await?;
    Ok(())
}

pub async fn get_interaction(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<UserInteractionStorageMetadata> {
    ctx.user_interaction
        .load(id)
        .await?
        .ok_or_else(|| not_found("user_interaction", id))
}

pub async fn list_interactions(
    ctx: &StorageContext,
    options: Option<UserInteractionListOptions>,
) -> crate::ApiResult<Vec<UserInteractionStorageMetadata>> {
    ctx.user_interaction.list(options).await.map_err(Into::into)
}

pub async fn list_interactions_by_execution(
    ctx: &StorageContext,
    execution_id: &str,
) -> crate::ApiResult<Vec<UserInteractionStorageMetadata>> {
    ctx.user_interaction
        .list_by_execution(execution_id)
        .await
        .map_err(Into::into)
}

pub async fn list_interactions_by_status(
    ctx: &StorageContext,
    status: &str,
) -> crate::ApiResult<Vec<UserInteractionStorageMetadata>> {
    ctx.user_interaction
        .list_by_status(status)
        .await
        .map_err(Into::into)
}

pub async fn get_interaction_stats(
    ctx: &StorageContext,
) -> crate::ApiResult<HashMap<String, u64>> {
    ctx.user_interaction.get_stats().await.map_err(Into::into)
}

pub async fn delete_interaction(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<bool> {
    ctx.user_interaction.delete(id).await.map_err(Into::into)
}

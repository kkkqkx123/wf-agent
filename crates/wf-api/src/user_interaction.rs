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

pub async fn get_interaction_stats(ctx: &StorageContext) -> crate::ApiResult<HashMap<String, u64>> {
    ctx.user_interaction.get_stats().await.map_err(Into::into)
}

pub async fn delete_interaction(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.user_interaction.delete(id).await.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_interaction(
        id: &str,
        execution_id: &str,
        interaction_type: &str,
        status: &str,
    ) -> UserInteractionStorageMetadata {
        UserInteractionStorageMetadata {
            id: id.into(),
            execution_id: execution_id.into(),
            interaction_type: interaction_type.into(),
            status: status.into(),
            request_data: json!({ "prompt": "confirm" }),
            response_data: None,
            result_data: None,
            error: None,
            created_at: 1000,
            responded_at: None,
        }
    }

    #[tokio::test]
    async fn interaction_crud() {
        let ctx = StorageContext::new_memory();
        save_interaction(
            &ctx,
            &make_interaction("ui-1", "ex-1", "confirm", "pending"),
        )
        .await
        .unwrap();

        let loaded = get_interaction(&ctx, "ui-1").await.unwrap();
        assert_eq!(loaded.interaction_type, "confirm");

        let err = get_interaction(&ctx, "ui-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_interaction(&ctx, "ui-1").await.unwrap());
        assert!(!delete_interaction(&ctx, "ui-1").await.unwrap());
    }

    #[tokio::test]
    async fn interaction_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_interaction(
            &ctx,
            &make_interaction("ui-1", "ex-1", "confirm", "pending"),
        )
        .await
        .unwrap();
        save_interaction(
            &ctx,
            &make_interaction("ui-2", "ex-1", "input", "responded"),
        )
        .await
        .unwrap();
        save_interaction(
            &ctx,
            &make_interaction("ui-3", "ex-2", "confirm", "pending"),
        )
        .await
        .unwrap();

        let by_execution = list_interactions_by_execution(&ctx, "ex-1").await.unwrap();
        assert_eq!(by_execution.len(), 2);

        let by_status = list_interactions_by_status(&ctx, "pending").await.unwrap();
        assert_eq!(by_status.len(), 2);

        let listed = list_interactions(&ctx, None).await.unwrap();
        assert_eq!(listed.len(), 3);

        let stats = get_interaction_stats(&ctx).await.unwrap();
        assert_eq!(stats.get("pending"), Some(&2));
        assert_eq!(stats.get("responded"), Some(&1));
    }
}

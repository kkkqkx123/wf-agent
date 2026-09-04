//! Draft versus formal agent lifecycle, mirroring workflow drafts.

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

pub use crate::workflow::draft::LifecycleStatus;

pub async fn lifecycle_of(ctx: &ApiContext, agent_id: &str) -> Option<LifecycleStatus> {
    if ctx
        .storage
        .agent_draft
        .exists(agent_id)
        .await
        .unwrap_or(false)
    {
        return Some(LifecycleStatus::Draft);
    }
    Some(LifecycleStatus::Formal)
}

pub async fn save_draft(
    ctx: &ApiContext,
    definition: &wf_types::agent::AgentDefinition,
) -> ApiResult<()> {
    if definition.id.trim().is_empty() {
        return Err(ApiError::Validation(
            "draft agent id must not be empty".into(),
        ));
    }
    if definition.name.trim().is_empty() {
        return Err(ApiError::Validation(
            "draft agent name must not be empty".into(),
        ));
    }
    ctx.storage.agent_draft.save(definition).await?;
    Ok(())
}

pub async fn get_draft(ctx: &ApiContext, id: &str) -> ApiResult<wf_types::agent::AgentDefinition> {
    ctx.storage
        .agent_draft
        .load(id)
        .await?
        .ok_or_else(|| ApiError::not_found("agent_draft", id))
}

pub async fn list_drafts(ctx: &ApiContext) -> ApiResult<Vec<wf_types::agent::AgentDefinition>> {
    ctx.storage.agent_draft.list(None).await.map_err(Into::into)
}

pub async fn delete_draft(ctx: &ApiContext, id: &str) -> ApiResult<bool> {
    Ok(ctx.storage.agent_draft.delete(id).await?)
}

/// Complete validation preview for publish, no persistence.
pub async fn validate_draft_complete(ctx: &ApiContext, id: &str) -> ApiResult<Vec<String>> {
    let draft = get_draft(ctx, id).await?;
    crate::agent::agent::validate_agent_definition(ctx, &draft).map_err(|e| match e {
        ApiError::Validation(detail) => ApiError::Validation(format!(
            "draft agent '{}' cannot be promoted: {}",
            id, detail
        )),
        other => other,
    })
}

/// Promote a draft agent to formal template registry.
pub async fn promote_draft(ctx: &ApiContext, id: &str) -> ApiResult<Vec<String>> {
    let draft = get_draft(ctx, id).await?;
    let warnings = crate::agent::agent::save_agent_template(ctx, &draft).await?;
    let _ = delete_draft(ctx, id).await;
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    fn draft_agent(id: &str) -> wf_types::agent::AgentDefinition {
        wf_types::agent::AgentDefinition {
            id: id.into(),
            name: format!("Draft {id}"),
            description: None,
            version: None,
            config: None,
            metadata: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn draft_agent_promotes_to_formal() {
        let ctx = make_ctx();
        save_draft(&ctx, &draft_agent("agent-draft")).await.unwrap();
        assert_eq!(
            lifecycle_of(&ctx, "agent-draft").await,
            Some(LifecycleStatus::Draft)
        );
        promote_draft(&ctx, "agent-draft").await.unwrap();
        assert!(get_draft(&ctx, "agent-draft").await.is_err());
        assert!(ctx.registries.agent_templates.has("agent-draft"));
    }
}

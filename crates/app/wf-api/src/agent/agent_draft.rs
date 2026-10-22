//! Draft versus formal agent lifecycle.
//!
//! An agent draft is an editable agent definition that promotes into the
//! formal agent template registry. The lifecycle itself is shared with every
//! other editable definition kind and lives in `crate::definition::draft`;
//! this module holds what is specific to an agent: its payload and its
//! parse-level check.

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

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

/// The drafts that belong to one agent definition.
///
/// A draft is a revision of one definition, so it is stored under that
/// definition's id; the drafts of a definition are the ones filed under
/// it.
pub async fn list_drafts_of(
    ctx: &ApiContext,
    definition_id: &str,
) -> ApiResult<Vec<wf_types::agent::AgentDefinition>> {
    Ok(list_drafts(ctx)
        .await?
        .into_iter()
        .filter(|draft| draft.id == definition_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
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
            crate::definition::draft::lifecycle_of(
                &ctx,
                crate::definition::draft::DefinitionKind::Agent,
                "agent-draft"
            )
            .await,
            Some(crate::definition::draft::LifecycleStatus::Draft)
        );
        crate::definition::draft::promote_draft(
            &ctx,
            crate::definition::draft::DefinitionKind::Agent,
            "agent-draft",
        )
        .await
        .unwrap();
        assert!(get_draft(&ctx, "agent-draft").await.is_err());
        assert!(ctx.registries.agent_templates.has("agent-draft"));
    }
}

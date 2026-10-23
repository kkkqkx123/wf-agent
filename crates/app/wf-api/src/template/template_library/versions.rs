use wf_core::registry::Registry;
use wf_storage::adapter::agent_template::AgentTemplateStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiResult};

use super::*;

pub async fn list_agent_template_versions(
    ctx: &ApiContext,
    id: &str,
) -> ApiResult<Vec<wf_types::agent::AgentTemplate>> {
    if !ctx.registries.agent_templates.has(id) {
        return Err(not_found("agent_template", id));
    }
    ctx.storage
        .agent_template
        .list_versions(id)
        .await
        .map_err(crate::ApiError::from)
}

pub async fn get_agent_template_version(
    ctx: &ApiContext,
    id: &str,
    version: &str,
) -> ApiResult<wf_types::agent::AgentTemplate> {
    ctx.storage
        .agent_template
        .load_version(id, version)
        .await
        .map_err(crate::ApiError::from)?
        .ok_or_else(|| not_found("agent_template_version", &format!("{}:v{}", id, version)))
}

pub async fn save_agent_template_version(
    ctx: &ApiContext,
    id: &str,
    version: &str,
    template: &wf_types::agent::AgentTemplate,
) -> ApiResult<()> {
    ctx.storage
        .agent_template
        .save_version(id, version, template)
        .await
        .map_err(crate::ApiError::from)
}

pub async fn rollback_agent_template(ctx: &ApiContext, id: &str, version: &str) -> ApiResult<()> {
    let template = get_agent_template_version(ctx, id, version).await?;
    if let Ok(current) = get_agent_template(ctx, id) {
        let base = current
            .definition
            .version
            .clone()
            .unwrap_or_else(|| format!("pre-rollback-{}", version));
        let label = match get_agent_template_version(ctx, id, &base).await {
            Err(crate::ApiError::NotFound { .. }) => base,
            Ok(_) => format!("{}-{}", base, wf_common::now()),
            Err(other) => return Err(other),
        };
        let _ = ctx
            .storage
            .agent_template
            .save_version(id, &label, &current)
            .await;
    }
    update_agent_template(ctx, &template).await?;
    Ok(())
}

use wf_core::registry::{MutableRegistry, Registry};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::agent::AgentTemplate;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

pub fn get_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<AgentTemplate> {
    ctx.registries
        .agent_templates
        .get(id)
        .map(|t| t.as_ref().clone())
        .ok_or_else(|| not_found("agent_template", id))
}

pub fn list_agent_templates(ctx: &ApiContext) -> ApiResult<Vec<AgentTemplate>> {
    let keys = ctx.registries.agent_templates.list();
    let mut templates = Vec::new();
    for key in keys {
        if let Some(template) = ctx.registries.agent_templates.get(&key) {
            templates.push(template.as_ref().clone());
        }
    }
    Ok(templates)
}

/// Validate an agent template definition against the live reference
/// context: shape plus profile plus tool closure. Errors reject the
/// write; warnings allow it. Shared by register, update and the
/// by-id validate endpoint so the library never persists a template its
/// own queries would flag as invalid.
pub async fn validate_agent_template_definition(
    ctx: &ApiContext,
    template: &AgentTemplate,
) -> ApiResult<Vec<wf_types::ValidationError>> {
    let val_ctx = crate::workflow::validation::build_reference_context(ctx).await;
    let result =
        crate::agent::validation::AgentValidator::new(&val_ctx).validate(&template.definition);
    if !result.is_valid() {
        let detail = result
            .errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ApiError::Validation(detail));
    }
    Ok(result.warnings)
}

pub async fn register_agent_template(ctx: &ApiContext, template: &AgentTemplate) -> ApiResult<()> {
    let registries = &ctx.registries;
    if registries.agent_templates.has(&template.id) {
        return Err(ApiError::already_exists(
            "agent_template",
            &template.id.to_string(),
        ));
    }
    validate_agent_template_definition(ctx, template)
        .await
        .map(|_| ())?;
    ctx.storage
        .agent_template
        .save(template)
        .await
        .map_err(crate::ApiError::from)?;
    registries
        .agent_templates
        .register(
            template.id.to_string(),
            std::sync::Arc::new(template.clone()),
        )
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    Ok(())
}

pub async fn delete_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.storage
        .agent_template
        .delete(id)
        .await
        .map_err(crate::ApiError::from)?;
    ctx.registries
        .agent_templates
        .unregister(id)
        .map(|_| ())
        .ok_or_else(|| not_found("agent_template", id))?;
    let _ = ctx.storage.template_usage.delete(id).await;
    Ok(())
}

/// Replace an agent template in place; errors with `NotFound` when the id
/// is not registered.
pub async fn update_agent_template(ctx: &ApiContext, template: &AgentTemplate) -> ApiResult<()> {
    if !ctx.registries.agent_templates.has(&template.id) {
        return Err(not_found("agent_template", &template.id.to_string()));
    }
    validate_agent_template_definition(ctx, template)
        .await
        .map(|_| ())?;
    ctx.storage
        .agent_template
        .save(template)
        .await
        .map_err(crate::ApiError::from)?;
    ctx.registries.agent_templates.register_or_replace(
        template.id.to_string(),
        std::sync::Arc::new(template.clone()),
    );
    Ok(())
}

/// Publish an agent template to the template library. Unset and explicit
/// publish marks both read as published, so pre-existing templates keep
/// their current featured/popular visibility.
pub async fn publish_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    let mut template = get_agent_template(ctx, id)?;
    template.is_published = Some(true);
    update_agent_template(ctx, &template).await
}

/// Remove the published mark from an agent template. The template stays
/// registered and queryable; it leaves the featured/popular selections.
pub async fn unpublish_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    let mut template = get_agent_template(ctx, id)?;
    template.is_published = Some(false);
    update_agent_template(ctx, &template).await
}

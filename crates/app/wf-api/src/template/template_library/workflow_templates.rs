use std::sync::Arc;

use wf_core::registry::{MutableRegistry, Registry};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::workflow::{WorkflowDefinition, WorkflowTemplate};

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

pub fn get_workflow_template(ctx: &ApiContext, id: &str) -> ApiResult<WorkflowTemplate> {
    ctx.registries
        .workflows
        .get(id)
        .map(|t| t.as_ref().clone())
        .ok_or_else(|| not_found("workflow_template", id))
}

pub fn list_workflow_templates(ctx: &ApiContext) -> ApiResult<Vec<WorkflowTemplate>> {
    let keys = ctx.registries.workflows.list();
    let mut templates = Vec::new();
    for key in keys {
        if let Some(template) = ctx.registries.workflows.get(&key) {
            templates.push(template.as_ref().clone());
        }
    }
    Ok(templates)
}

/// The definition a workflow template carries.
///
/// The library facts the template owns — the category and tags it is
/// listed under, whether it is published, and the governance flags the
/// library enforces — travel with the definition, because the definition
/// is the only form that outlives a restart. The stored definition and the
/// registry entry therefore hold the same document.
fn template_definition(template: &WorkflowTemplate, published: bool) -> WorkflowDefinition {
    let mut definition = template.definition.clone();
    let metadata = definition.metadata.get_or_insert_with(Default::default);
    if template.template_category.is_some() {
        metadata.category = template.template_category.clone();
    }
    if template.template_tags.is_some() {
        metadata.tags = template.template_tags.clone();
    }
    metadata.published = Some(published);
    if template.is_public.is_some() {
        metadata.is_public = template.is_public;
    }
    if template.is_system.is_some() {
        metadata.is_system = template.is_system;
    }
    if template.enabled.is_some() {
        metadata.enabled = template.enabled;
    }
    definition
}

/// The registry entry a workflow template registers under.
///
/// The entry follows the definition: the library facts it carries are the
/// ones the definition states, and a fact the definition leaves open keeps
/// the one the template states.
fn template_entry(template: &WorkflowTemplate, definition: WorkflowDefinition) -> WorkflowTemplate {
    let facts = crate::workflow::library_facts(definition.metadata.as_ref(), Some(template));
    let mut entry = template.clone();
    entry.definition = definition;
    entry.is_public = facts.is_public;
    entry.is_system = facts.is_system;
    entry.enabled = facts.enabled;
    entry.is_published = Some(facts.is_published);
    entry
}

/// Register a workflow template; errors with `AlreadyExists` when the id
/// is already registered.
///
/// The template's definition is stored with it, so a registered template
/// survives a restart and stays visible to the template library.
pub async fn register_workflow_template(
    ctx: &ApiContext,
    template: &WorkflowTemplate,
) -> ApiResult<()> {
    let registries = &ctx.registries;
    if registries.workflows.has(&template.id) {
        return Err(ApiError::already_exists(
            "workflow_template",
            &template.id.to_string(),
        ));
    }
    let mut definition = template_definition(template, true);
    let stored = ctx.storage.workflow.load(&template.id).await?;
    crate::workflow::carry_metadata(stored.as_ref(), &mut definition);
    ctx.storage.workflow.save(&definition).await?;
    let entry = template_entry(template, definition);
    registries
        .workflows
        .register(entry.id.to_string(), Arc::new(entry))
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    ctx.clear_stale(&template.id.to_string());
    Ok(())
}

/// Remove a workflow template from the library.
///
/// The template is the user's own workflow, published: removing it from
/// the library must not destroy the workflow. The published mark is
/// cleared, so the workflow does not come back as a library item, and the
/// entry leaves the registry. The workflow itself stays where it is and
/// can be published again.
pub async fn delete_workflow_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.registries
        .workflows
        .unregister(id)
        .map(|_| ())
        .ok_or_else(|| not_found("workflow_template", id))?;
    clear_published(ctx, id).await?;
    let _ = ctx.storage.template_usage.delete(id).await;
    Ok(())
}

/// Replace a workflow template in place; errors with `NotFound` when the
/// id is not registered.
///
/// The stored definition is replaced with the template's, so the library
/// entry and the workflow the runtime executes stay one document.
pub async fn update_workflow_template(
    ctx: &ApiContext,
    template: &WorkflowTemplate,
) -> ApiResult<()> {
    if !ctx.registries.workflows.has(&template.id) {
        return Err(not_found("workflow_template", &template.id.to_string()));
    }
    let mut definition = template_definition(template, true);
    let stored = ctx.storage.workflow.load(&template.id).await?;
    crate::workflow::carry_metadata(stored.as_ref(), &mut definition);
    ctx.storage.workflow.save(&definition).await?;
    let entry = template_entry(template, definition);
    ctx.registries
        .workflows
        .register_or_replace(entry.id.to_string(), Arc::new(entry));
    ctx.clear_stale(&template.id.to_string());
    Ok(())
}

/// Publish a workflow definition.
pub async fn publish_workflow_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    crate::workflow::publish_workflow(ctx, id).await
}

/// Remove the published mark from a workflow definition.
pub async fn unpublish_workflow_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    crate::workflow::unpublish_workflow(ctx, id).await
}

/// Clear the published mark of a workflow in the stored definition.
async fn clear_published(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    let Some(mut definition) = ctx.storage.workflow.load(id).await? else {
        return Ok(());
    };
    definition
        .metadata
        .get_or_insert_with(Default::default)
        .published = Some(false);
    ctx.storage.workflow.save(&definition).await?;
    Ok(())
}

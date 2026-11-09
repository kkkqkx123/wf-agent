use std::sync::Arc;

use wf_core::registry::{MutableRegistry, Registry};
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::agent::{AgentDefinition, AgentMetadata, AgentTemplate};

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

/// Library facts of an agent, derived from its definition metadata with
/// the registered view as fallback. Mirrors the workflow library facts so
/// the two definition kinds cannot drift apart.
pub struct AgentLibraryFacts {
    pub is_published: bool,
    pub is_public: Option<bool>,
    pub is_system: Option<bool>,
    pub enabled: Option<bool>,
}

pub fn library_facts(
    metadata: Option<&AgentMetadata>,
    entry: Option<&AgentTemplate>,
) -> AgentLibraryFacts {
    AgentLibraryFacts {
        is_published: metadata
            .and_then(|m| m.published)
            .or_else(|| entry.and_then(|t| t.is_published))
            .unwrap_or(false),
        is_public: metadata
            .and_then(|m| m.is_public)
            .or_else(|| entry.and_then(|t| t.is_public)),
        is_system: metadata
            .and_then(|m| m.is_system)
            .or_else(|| entry.and_then(|t| t.is_system)),
        enabled: metadata
            .and_then(|m| m.enabled)
            .or_else(|| entry.and_then(|t| t.enabled))
            .or(Some(true)),
    }
}

/// The registry view composed from an agent definition.
///
/// The definition row is the only persisted form of an agent; the library
/// facts come from its metadata, and a fact the metadata leaves open keeps
/// the one the registered view already carries.
pub fn library_entry(
    registries: &Arc<ResourceRegistries>,
    definition: &AgentDefinition,
) -> AgentTemplate {
    let existing = registries.agent_templates.get(&definition.id.to_string());
    let metadata = definition.metadata.as_ref();
    let facts = library_facts(metadata, existing.as_deref());
    AgentTemplate {
        id: definition.id.clone(),
        name: definition.name.clone(),
        description: definition.description.clone().unwrap_or_default(),
        definition: definition.clone(),
        template_category: metadata.and_then(|m| m.category.clone()),
        template_tags: metadata.and_then(|m| m.tags.clone()),
        is_public: facts.is_public,
        is_system: facts.is_system,
        enabled: facts.enabled,
        is_published: Some(facts.is_published),
    }
}

/// The definition a registered agent template persists.
///
/// The library facts the template carries travel with the definition,
/// because the definition is the only form that outlives a restart. A fact
/// the wrapper leaves open keeps the one the definition states. Identity
/// fields flow from the wrapper into the definition so a rename through
/// the template write lands on the persisted row.
fn template_definition(template: &AgentTemplate) -> AgentDefinition {
    let mut definition = template.definition.clone();
    definition.id = template.id.clone();
    definition.name = template.name.clone();
    if !template.description.is_empty() {
        definition.description = Some(template.description.clone());
    }
    let metadata = definition.metadata.get_or_insert_with(Default::default);
    if template.template_category.is_some() {
        metadata.category = template.template_category.clone();
    }
    if template.template_tags.is_some() {
        metadata.tags = template.template_tags.clone();
    }
    if template.is_published.is_some() {
        metadata.published = template.is_published;
    }
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

/// Persist an agent definition and refresh its registry view. The single
/// write path every agent mutation funnels through.
async fn persist_definition(ctx: &ApiContext, definition: &AgentDefinition) -> ApiResult<()> {
    ctx.storage
        .agent
        .save(definition)
        .await
        .map_err(crate::ApiError::from)?;
    let entry = library_entry(&ctx.registries, definition);
    ctx.registries
        .agent_templates
        .register_or_replace(entry.id.to_string(), Arc::new(entry));
    Ok(())
}

/// Validate a bare definition through the template gate by probing it with
/// no wrapper facts, so definition-only writes face the same gate as
/// template writes.
async fn validate_definition(ctx: &ApiContext, definition: &AgentDefinition) -> ApiResult<()> {
    let probe = AgentTemplate {
        id: definition.id.clone(),
        name: definition.name.clone(),
        description: definition.description.clone().unwrap_or_default(),
        definition: definition.clone(),
        template_category: None,
        template_tags: None,
        is_public: None,
        is_system: None,
        enabled: None,
        is_published: None,
    };
    validate_agent_template_definition(ctx, &probe)
        .await
        .map(|_| ())
}

/// Replace the persisted definition of an agent in place. The stored row
/// is the gate: a definition that was never stored cannot be updated.
/// A missing registry view is rebuilt by the persist path, so storage
/// existence alone allows the write.
pub async fn update_agent_definition(
    ctx: &ApiContext,
    definition: &AgentDefinition,
) -> ApiResult<()> {
    load_definition(ctx, &definition.id).await?;
    validate_definition(ctx, definition).await?;
    persist_definition(ctx, definition).await
}

/// Load the persisted definition behind an agent. Crate-visible so
/// version rollback snapshots the stored row rather than the registry view.
pub(crate) async fn load_definition(ctx: &ApiContext, id: &str) -> ApiResult<AgentDefinition> {
    ctx.storage
        .agent
        .load(id)
        .await
        .map_err(crate::ApiError::from)?
        .ok_or_else(|| not_found("agent_template", id))
}

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
    let definition = template_definition(template);
    persist_definition(ctx, &definition).await
}

pub async fn delete_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.storage
        .agent
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
    let definition = template_definition(template);
    persist_definition(ctx, &definition).await
}

/// Publish an agent to the template library. The published mark is written
/// to the persisted definition, so it stays published across a restart.
pub async fn publish_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    let mut definition = load_definition(ctx, id).await?;
    definition
        .metadata
        .get_or_insert_with(Default::default)
        .published = Some(true);
    update_agent_definition(ctx, &definition).await
}

/// Remove the published mark from an agent definition. The agent stays
/// registered and queryable; it leaves the featured/popular selections.
pub async fn unpublish_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    let mut definition = load_definition(ctx, id).await?;
    definition
        .metadata
        .get_or_insert_with(Default::default)
        .published = Some(false);
    update_agent_definition(ctx, &definition).await
}

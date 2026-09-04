use wf_core::registry::{MutableRegistry, Registry};
use wf_storage::adapter::agent_execution::{
    AgentExecutionListOptions, AgentExecutionStorageAdapter,
};
use wf_storage::adapter::agent_loop::{AgentLoopListOptions, AgentLoopStorageAdapter};
use wf_storage::adapter::agent_profile::AgentProfileListOptions;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::{AgentExecution, AgentLoopStorageMetadata, AgentProfileStorageMetadata};

use crate::infra::context::ApiContext;
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

/// Formal validation of an agent definition: shape plus profile existence
/// plus tool existence with enabled-state distinction. Returns warnings;
/// errors reject the definition.
pub fn validate_agent_definition(
    ctx: &ApiContext,
    definition: &wf_types::agent::AgentDefinition,
) -> crate::ApiResult<Vec<String>> {
    let val_ctx = wf_types::ValidationContext::empty();
    let validator = super::validation::AgentValidator::new(&val_ctx);
    let result = validator.validate(definition);

    if !result.is_valid() {
        let detail: Vec<String> = result
            .errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect();
        return Err(crate::ApiError::Validation(detail.join("; ")));
    }

    // Additional template existence check (not in shared validator).
    if let Some(template_id) = definition
        .config
        .as_ref()
        .and_then(|c| c.system_prompt_template_id.as_ref())
    {
        if !ctx.registries.templates.has(template_id) {
            return Err(crate::ApiError::Validation(format!(
                "agent '{}' references prompt template '{}' which is not registered",
                definition.id, template_id
            )));
        }
    }

    // Return warnings as strings.
    let warnings: Vec<String> = result
        .warnings
        .into_iter()
        .map(|w| format!("agent '{}': {}", definition.id, w.message))
        .collect();
    Ok(warnings)
}

/// Formal save of an agent definition into the template registry.
/// Shape plus reference closure must pass; warnings allow registration.
/// The template is persisted before being registered so restarts keep it.
pub async fn save_agent_template(
    ctx: &ApiContext,
    definition: &wf_types::agent::AgentDefinition,
) -> crate::ApiResult<Vec<String>> {
    let warnings = validate_agent_definition(ctx, definition)?;
    let template = wf_types::agent::AgentTemplate {
        id: definition.id.clone(),
        name: definition.name.clone(),
        description: definition.description.clone().unwrap_or_default(),
        definition: definition.clone(),
        template_category: None,
        template_tags: None,
        is_public: None,
        enabled: None,
    };
    ctx.storage
        .agent_template
        .save(&template)
        .await
        .map_err(crate::ApiError::from)?;
    if ctx.registries.agent_templates.has(&template.id.to_string()) {
        ctx.registries
            .agent_templates
            .unregister(&template.id.to_string());
    }
    ctx.registries
        .agent_templates
        .register(template.id.to_string(), std::sync::Arc::new(template))
        .map_err(|e| crate::ApiError::Conflict(e.to_string()))?;
    Ok(warnings)
}

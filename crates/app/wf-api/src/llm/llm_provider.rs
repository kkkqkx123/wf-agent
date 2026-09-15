//! LLM provider definitions (connection templates) and model discovery.
//!
//! Provider definitions carry the connection defaults shared by multiple
//! profiles (base URL, auth, headers, model discovery) and reference one
//! wire-protocol format by name. Profiles point at them through
//! `LlmProfile::provider_id`; explicit profile fields win at registration
//! time. Model listing runs off the hot path through the gateway
//! `ModelCatalog` and never blocks profile registration: discovery
//! failures degrade to warnings.

use wf_types::llm::{LlmProfile, LlmProviderDefinition, ModelInfo};

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

/// List all registered provider definitions.
pub async fn list(ctx: &ApiContext) -> ApiResult<Vec<LlmProviderDefinition>> {
    Ok(ctx.llm_gateway.provider_registry().list())
}

/// Get one provider definition by id.
pub async fn get(ctx: &ApiContext, id: &str) -> ApiResult<LlmProviderDefinition> {
    ctx.llm_gateway
        .provider_registry()
        .get(id)
        .ok_or_else(|| not_found("provider", id))
}

/// Register (or replace) a provider definition. Cached gateway clients
/// are evicted so profiles referencing it pick up the new defaults.
pub async fn create(ctx: &ApiContext, definition: &LlmProviderDefinition) -> ApiResult<()> {
    if definition.id.trim().is_empty() {
        return Err(ApiError::Validation("provider id must not be empty".into()));
    }
    if let wf_types::llm::LlmFormat::Custom(name) = &definition.format {
        if name.trim().is_empty() {
            return Err(ApiError::Validation(format!(
                "provider '{}' format must not be empty",
                definition.id
            )));
        }
    }
    ctx.llm_gateway
        .register_provider_definition(definition.clone())
        .map_err(ApiError::from)?;
    Ok(())
}

/// Remove a provider definition and evict cached clients. Profiles that
/// referenced it keep their merged snapshot.
pub async fn delete(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.llm_gateway
        .remove_provider_definition(id)
        .map(|_| ())
        .ok_or_else(|| not_found("provider", id))
}

/// List the models advertised by a provider definition.
///
/// The effective API key is taken from the first profile referencing the
/// provider (when any); definitions without remote discovery return
/// without using it. Discovery failures surface as errors to the caller;
/// assembly-time checks degrade them to warnings instead.
pub async fn list_models(ctx: &ApiContext, provider_id: &str) -> ApiResult<Vec<ModelInfo>> {
    let definition = get(ctx, provider_id).await?;
    let api_key = ctx
        .llm_gateway
        .profile_registry()
        .list()
        .into_iter()
        .find(|p| p.provider_id.as_deref() == Some(provider_id))
        .and_then(|p| p.api_key);
    ctx.llm_gateway
        .model_catalog()
        .list_models(&definition, api_key.as_deref())
        .await
        .map_err(ApiError::from)
}

/// Assembly-time check of a profile against its provider catalog.
///
/// - Discovery failure: warn and proceed, never block registration.
/// - Model missing from a non-empty catalog: warn (stale or custom model).
/// - Catalog entry carries `context_window_size` while the profile has
///   none: backfill it and return the enriched profile.
pub async fn assemble_check(ctx: &ApiContext, profile: &LlmProfile) -> LlmProfile {
    let Some(provider_id) = profile.provider_id.as_deref() else {
        return profile.clone();
    };
    let models = match list_models(ctx, provider_id).await {
        Ok(models) => models,
        Err(e) => {
            tracing::warn!(
                profile_id = %profile.id,
                provider = %provider_id,
                "model discovery failed, skipping assembly check: {e}"
            );
            return profile.clone();
        }
    };
    if !models.is_empty() && !models.iter().any(|m| m.id == profile.model) {
        tracing::warn!(
            profile_id = %profile.id,
            provider = %provider_id,
            model = %profile.model,
            "model not advertised by provider catalog"
        );
    }
    let mut enriched = profile.clone();
    if enriched.context_window_size.is_none() {
        if let Some(entry) = models.iter().find(|m| m.id == profile.model) {
            enriched.context_window_size = entry.context_window_size;
        }
    }
    enriched
}

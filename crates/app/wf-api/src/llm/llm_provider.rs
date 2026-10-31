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
/// Collect stored proxy credentials for every distinct proxy route in use.
///
/// Keys follow the shared route-derived naming so discovery composes the
/// transient address without reading the store itself. Missing entries mean
/// anonymous routes and are simply absent from the map.
async fn proxy_creds(
    ctx: &ApiContext,
) -> std::collections::HashMap<String, wf_llm::ProxyCredential> {
    let Some(registry) = ctx.llm_gateway.auth_registry() else {
        return Default::default();
    };
    let mut routes = std::collections::HashSet::new();
    for definition in ctx.llm_gateway.provider_registry().list() {
        if let Some(proxy) = definition.proxy.as_deref() {
            routes.insert(proxy.to_string());
        }
    }
    for profile in ctx.llm_gateway.profile_registry().list() {
        if let Some(proxy) = profile.proxy.as_deref() {
            routes.insert(proxy.to_string());
        }
    }
    let mut out = std::collections::HashMap::new();
    for route in routes {
        let key = wf_llm::credential_key(&route);
        if let Ok(Some(credential)) = registry.store().read(&key).await {
            if let Ok(proxy) = credential.as_proxy() {
                out.insert(key, proxy.clone());
            }
        }
    }
    out
}

pub async fn list_models(ctx: &ApiContext, provider_id: &str) -> ApiResult<Vec<ModelInfo>> {
    let definition = get(ctx, provider_id).await?;
    let api_key = ctx
        .llm_gateway
        .profile_registry()
        .list()
        .into_iter()
        .find(|p| p.provider_id.as_deref() == Some(provider_id))
        .and_then(|p| p.api_key);
    let proxy_creds = proxy_creds(ctx).await;
    ctx.llm_gateway
        .model_catalog()
        .list_models(&definition, api_key.as_deref(), &proxy_creds)
        .await
        .map_err(ApiError::from)
}

/// List the models advertised by a provider, credential-filtered.
///
/// Same key resolution as [`list_models`], then `predicate` keeps only the
/// entries the logged-in credential may use (allow-list, prefix, or
/// subscription flag closed over by the caller). The predicate is a plain
/// function so no trait object crosses the API boundary.
pub async fn list_models_filtered(
    ctx: &ApiContext,
    provider_id: &str,
    predicate: impl Fn(&ModelInfo) -> bool,
) -> ApiResult<Vec<ModelInfo>> {
    let models = list_models(ctx, provider_id).await?;
    Ok(wf_llm::filter_models(models, predicate))
}

/// List the models advertised by a provider, restricted to an allow-list.
///
/// Convenience over [`list_models_filtered`] for the credential-filtering
/// shape where the login owns an explicit set of model ids. An empty
/// allow-list yields an empty listing.
pub async fn list_models_for_allowlist(
    ctx: &ApiContext,
    provider_id: &str,
    allowed: &std::collections::HashSet<String>,
) -> ApiResult<Vec<ModelInfo>> {
    let models = list_models(ctx, provider_id).await?;
    Ok(wf_llm::filter_models_by_allowlist(models, allowed))
}

/// Refresh the model listings of every registered provider at once.
///
/// Transactional in the reporting sense: every provider yields an entry,
/// successes carry the discovered models and failures carry the discovery
/// error. Callers degrade failures to warnings; one unreachable endpoint
/// never blocks the rest.
pub async fn refresh_all(
    ctx: &ApiContext,
) -> ApiResult<std::collections::HashMap<String, Result<Vec<ModelInfo>, String>>> {
    let definitions = ctx.llm_gateway.provider_registry().list();
    let api_keys: std::collections::HashMap<String, String> = ctx
        .llm_gateway
        .profile_registry()
        .list()
        .into_iter()
        .filter_map(|p| {
            let provider_id = p.provider_id.clone()?;
            let api_key = p.api_key.clone()?;
            Some((provider_id, api_key))
        })
        .collect();
    let proxy_creds = proxy_creds(ctx).await;
    let refreshed = ctx
        .llm_gateway
        .model_catalog()
        .refresh_all(&definitions, &api_keys, &proxy_creds)
        .await;
    Ok(refreshed
        .into_iter()
        .map(|(id, result)| (id, result.map_err(|e| e.to_string())))
        .collect())
}

/// Refresh every provider listing, then credential-filter the successes.
///
/// Same reporting as [`refresh_all`], with `predicate` applied to each
/// successful listing. Failures pass through untouched so callers can warn
/// per provider instead of failing the whole refresh.
pub async fn refresh_filtered(
    ctx: &ApiContext,
    predicate: impl Fn(&ModelInfo) -> bool,
) -> ApiResult<std::collections::HashMap<String, Result<Vec<ModelInfo>, String>>> {
    let definitions = ctx.llm_gateway.provider_registry().list();
    let api_keys: std::collections::HashMap<String, String> = ctx
        .llm_gateway
        .profile_registry()
        .list()
        .into_iter()
        .filter_map(|p| {
            let provider_id = p.provider_id.clone()?;
            let api_key = p.api_key.clone()?;
            Some((provider_id, api_key))
        })
        .collect();
    let proxy_creds = proxy_creds(ctx).await;
    let refreshed = ctx
        .llm_gateway
        .model_catalog()
        .refresh_filtered(&definitions, &api_keys, &proxy_creds, predicate)
        .await;
    Ok(refreshed
        .into_iter()
        .map(|(id, result)| (id, result.map_err(|e| e.to_string())))
        .collect())
}

/// Assembly-time check of a profile against its provider catalog.
///
/// - Discovery failure: warn and proceed, never block registration.
/// - Model missing from a non-empty catalog: warn (stale or custom model).
/// - Catalog entry carries `context_window_size`/`max_output_tokens` while
///   the profile has none: backfill the missing gaps and return the enriched
///   profile. The live discovery table only carries the context window, so
///   a still-missing `max_output_tokens` falls back to the static LiteLLM
///   table (provider-aware `wf_llm::model_metadata_for_provider`);
///   `format` is never auto-filled.
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
    if enriched.max_output_tokens.is_none() {
        let definition = ctx.llm_gateway.provider_registry().get(provider_id);
        let custom = definition.as_ref().and_then(wf_llm::custom_litellm_of);
        let namespace = wf_llm::resolve_litellm_namespace(provider_id, custom);
        if let Some(meta) = wf_llm::model_metadata_for_namespace(namespace, &profile.model) {
            if enriched.context_window_size.is_none() {
                enriched.context_window_size = meta.context_window;
            }
            enriched.max_output_tokens = meta.max_output_tokens;
        }
    }
    enriched
}

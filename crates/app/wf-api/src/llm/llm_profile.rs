//! LLM profile resource management.
//!
//! Backed by the shared `LlmGateway` profile registry, so profiles managed
//! through this API are the same profiles every LLM request resolves.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use wf_llm::profile_manager::validate_profile;
use wf_types::llm::{LlmProfile, LlmProvider};

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

/// Mask placeholder used for exported API keys, so a masked export cannot
/// round-trip a key.
pub const MASKED_API_KEY: &str = "***HIDDEN***";

/// One point of an LLM profile template: a partial profile whose id/name/key
/// are filled by the caller at creation time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProfileTemplate {
    pub name: String,
    pub description: String,
    pub profile: LlmProfile,
}

/// Profile filter.
#[derive(Debug, Clone, Default)]
pub struct LlmProfileFilter {
    pub id: Option<String>,
    pub name: Option<String>,
    pub provider: Option<LlmProvider>,
    pub model: Option<String>,
}

/// Register a profile; errors with `AlreadyExists` when the id is taken.
pub async fn create(ctx: &ApiContext, profile: &LlmProfile) -> ApiResult<()> {
    let manager = ctx.llm_gateway.profile_registry();
    if manager.has(&profile.id) {
        return Err(ApiError::already_exists("profile", &profile.id));
    }
    validate_profile(profile)?;
    manager.register(profile.clone())?;
    Ok(())
}

/// Replace a profile with the given complete profile (must carry the
/// target id). The idiomatic update form: load, modify and pass the full
/// record back.
pub async fn update(ctx: &ApiContext, profile: &LlmProfile) -> ApiResult<()> {
    let manager = ctx.llm_gateway.profile_registry();
    if !manager.has(&profile.id) {
        return Err(not_found("profile", &profile.id));
    }
    validate_profile(profile)?;
    manager.register(profile.clone())?;
    Ok(())
}

/// Update a profile and report the impact on dependent workflows and agent
/// templates. The update always applies; dependents that now fail formal
/// validation are reported.
pub async fn update_with_impact(
    ctx: &ApiContext,
    profile: &LlmProfile,
) -> ApiResult<crate::infra::dependency::UpdateImpactReport> {
    update(ctx, profile).await?;
    crate::infra::dependency::check_update_impact(
        ctx,
        crate::infra::dependency::DependencyKind::Profile,
        &profile.id,
    )
    .await
}

/// Get a profile by id; an empty id resolves to the default profile.
pub async fn get(ctx: &ApiContext, id: &str) -> ApiResult<LlmProfile> {
    ctx.llm_gateway
        .profile_registry()
        .get(id)
        .ok_or_else(|| not_found("profile", id))
}

/// List all registered profiles.
pub async fn list(ctx: &ApiContext) -> ApiResult<Vec<LlmProfile>> {
    Ok(ctx.llm_gateway.profile_registry().list())
}

/// Delete a profile; errors with `NotFound` when it does not exist.
pub async fn delete(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.llm_gateway
        .remove_profile(id)
        .map(|_| ())
        .ok_or_else(|| not_found("profile", id))
}

/// Filter the profile list by id/name/provider/model.
pub async fn query(ctx: &ApiContext, filter: &LlmProfileFilter) -> ApiResult<Vec<LlmProfile>> {
    let profiles = list(ctx).await?;
    Ok(profiles
        .into_iter()
        .filter(|p| {
            if let Some(id) = &filter.id {
                if !p.id.contains(id.as_str()) {
                    return false;
                }
            }
            if let Some(name) = &filter.name {
                if !p.name.contains(name.as_str()) {
                    return false;
                }
            }
            if let Some(provider) = &filter.provider {
                if p.provider != *provider {
                    return false;
                }
            }
            if let Some(model) = &filter.model {
                if !p.model.contains(model.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect())
}

// ── default profile ─────────────────────────────────────────────

/// Set the default profile (the first registered profile is the implicit
/// default until one is set explicitly).
pub async fn set_default(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.llm_gateway.profile_registry().set_default(id)?;
    Ok(())
}

pub async fn get_default(ctx: &ApiContext) -> ApiResult<Option<LlmProfile>> {
    Ok(ctx.llm_gateway.profile_registry().get_default())
}

pub async fn get_default_id(ctx: &ApiContext) -> ApiResult<Option<String>> {
    Ok(ctx.llm_gateway.profile_registry().default_id())
}

// ── export / import (key masking) ───────────────────────────────

/// Export a profile with the API key masked. The masked value
/// (`***HIDDEN***`) is rejected by [`import_json`].
pub async fn export(ctx: &ApiContext, id: &str) -> ApiResult<Value> {
    let profile = get(ctx, id).await?;
    Ok(mask_profile(&profile))
}

/// Export all profiles with API keys masked.
pub async fn export_all(ctx: &ApiContext) -> ApiResult<Vec<Value>> {
    let profiles = list(ctx).await?;
    Ok(profiles.iter().map(mask_profile).collect())
}

/// Export one profile as a pretty JSON string (masked).
pub async fn export_json(ctx: &ApiContext, id: &str) -> ApiResult<String> {
    let value = export(ctx, id).await?;
    Ok(serde_json::to_string_pretty(&value)?)
}

/// Export all profiles as a pretty JSON array (masked).
pub async fn export_all_json(ctx: &ApiContext) -> ApiResult<String> {
    let value = export_all(ctx).await?;
    Ok(serde_json::to_string_pretty(&value)?)
}

/// Import one profile from a JSON string; rejects exports whose key is
/// still masked. Returns the imported profile id.
pub async fn import_json(ctx: &ApiContext, json: &str) -> ApiResult<String> {
    let profile: LlmProfile =
        serde_json::from_str(json).map_err(|e| ApiError::Validation(e.to_string()))?;
    if profile.api_key.as_deref() == Some(MASKED_API_KEY) {
        return Err(ApiError::Validation(
            "Cannot import a profile with a masked API key; supply the real key".into(),
        ));
    }
    create(ctx, &profile).await?;
    Ok(profile.id)
}

/// Import several profiles from a JSON array string; returns the ids of
/// all successfully imported profiles.
pub async fn import_all_json(ctx: &ApiContext, json: &str) -> ApiResult<Vec<String>> {
    let profiles: Vec<LlmProfile> =
        serde_json::from_str(json).map_err(|e| ApiError::Validation(e.to_string()))?;
    let mut ids = Vec::new();
    for profile in profiles {
        if profile.api_key.as_deref() == Some(MASKED_API_KEY) {
            continue;
        }
        if create(ctx, &profile).await.is_ok() {
            ids.push(profile.id);
        }
    }
    Ok(ids)
}

/// Validate a profile without registering or persisting it.
///
/// Delegates to the shared `wf_llm::profile_manager::validate_profile` so
/// the verdict always agrees with `create` / `update` (which route through
/// the same check). `api_key` is intentionally not required — it may be
/// injected per request (see `wf-llm`).
pub fn validate(_ctx: &ApiContext, profile: &LlmProfile) -> (bool, Vec<String>) {
    match validate_profile(profile) {
        Ok(()) => (true, Vec::new()),
        Err(err) => (false, vec![err.to_string()]),
    }
}

// ── templates ───────────────────────────────────────────────────

/// Well-known persistence key holding the runtime custom profile templates.
const CUSTOM_TEMPLATES_KEY: &str = "custom:llm_profile_templates";

async fn load_custom_templates(ctx: &ApiContext) -> Vec<LlmProfileTemplate> {
    ctx.persistence
        .load_snapshot(CUSTOM_TEMPLATES_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value::<Vec<LlmProfileTemplate>>(value).ok())
        .unwrap_or_default()
}

/// List the built-in profile templates plus any custom templates registered
/// through [`add_template`].
pub async fn list_templates(ctx: &ApiContext) -> ApiResult<Vec<LlmProfileTemplate>> {
    let mut templates = builtin_templates();
    templates.extend(load_custom_templates(ctx).await);
    Ok(templates)
}

pub async fn get_template(ctx: &ApiContext, name: &str) -> ApiResult<Option<LlmProfileTemplate>> {
    Ok(list_templates(ctx)
        .await?
        .into_iter()
        .find(|t| t.name == name))
}

/// Register a runtime custom template (persisted through the context's
/// persistence layer). The name must be unique across the built-in and
/// custom templates.
pub async fn add_template(ctx: &ApiContext, template: LlmProfileTemplate) -> ApiResult<()> {
    if template.name.trim().is_empty() {
        return Err(ApiError::Validation("template name is required".into()));
    }
    let mut custom = load_custom_templates(ctx).await;
    if builtin_templates().iter().any(|t| t.name == template.name)
        || custom.iter().any(|t| t.name == template.name)
    {
        return Err(ApiError::already_exists("template", &template.name));
    }
    custom.push(template);
    ctx.persistence
        .save_snapshot(CUSTOM_TEMPLATES_KEY, &serde_json::to_value(custom)?)
        .await?;
    Ok(())
}

/// Remove a runtime custom template by name. Built-in templates cannot be
/// removed. Returns whether a template was removed.
pub async fn remove_template(ctx: &ApiContext, name: &str) -> ApiResult<bool> {
    let mut custom = load_custom_templates(ctx).await;
    let before = custom.len();
    custom.retain(|t| t.name != name);
    if custom.len() == before {
        return Ok(false);
    }
    ctx.persistence
        .save_snapshot(CUSTOM_TEMPLATES_KEY, &serde_json::to_value(custom)?)
        .await?;
    Ok(true)
}

/// Create a profile from a template: start from the template profile and
/// overlay the caller's overrides (JSON object of `LlmProfile` fields;
/// `id`/`name` fall back to a generated id / the template name).
pub async fn create_from_template(
    ctx: &ApiContext,
    template_name: &str,
    overrides: &Value,
) -> ApiResult<String> {
    let template = get_template(ctx, template_name)
        .await?
        .ok_or_else(|| not_found("template", template_name))?;

    let mut base = serde_json::to_value(&template.profile)?;
    if let Value::Object(map) = &mut base {
        if let Some(overrides) = overrides.as_object() {
            for (key, value) in overrides {
                map.insert(key.clone(), value.clone());
            }
        }
    }

    let generated_id = format!("profile-{}", wf_common::now());
    let name = overrides
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| template.name.clone());
    let base_map = base.as_object_mut().expect("profile serializes to object");
    if base_map
        .get("id")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        base_map.insert("id".to_string(), json!(generated_id));
    }
    if base_map
        .get("name")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        base_map.insert("name".to_string(), json!(name));
    }

    let profile: LlmProfile =
        serde_json::from_value(base).map_err(|e| ApiError::Validation(e.to_string()))?;
    create(ctx, &profile).await?;
    Ok(profile.id)
}

/// Mask the api key of a serialized profile.
fn mask_profile(profile: &LlmProfile) -> Value {
    let mut value = serde_json::to_value(profile).unwrap_or_else(|_| json!({}));
    if let Some(map) = value.as_object_mut() {
        if map.contains_key("api_key") {
            map.insert("api_key".into(), json!(MASKED_API_KEY));
        }
    }
    value
}

fn template_profile(
    provider: LlmProvider,
    model: &str,
    generation: wf_types::llm::generation::LlmGenerationParams,
) -> LlmProfile {
    LlmProfile {
        id: String::new(),
        name: String::new(),
        provider,
        model: model.to_string(),
        api_key: None,
        base_url: None,
        parameters: None,
        generation: Some(generation),
        timeout: None,
        max_retries: None,
        retry_delay: None,
        headers: None,
        metadata: None,
        tool_call_format: None,
        auth_type: None,
        custom_headers: None,
        custom_body: None,
        custom_body_enabled: None,
        query_params: None,
        stream_options: None,
        context_window_size: None,
    }
}

fn builtin_templates() -> Vec<LlmProfileTemplate> {
    use wf_types::llm::generation::LlmGenerationParams;
    vec![
        LlmProfileTemplate {
            name: "openai-chat".into(),
            description: "OpenAI Chat API configuration template".into(),
            profile: template_profile(
                LlmProvider::OpenaiChat,
                "gpt-5",
                LlmGenerationParams {
                    temperature: Some(0.7),
                    max_tokens: Some(8192),
                    ..Default::default()
                },
            ),
        },
        LlmProfileTemplate {
            name: "anthropic".into(),
            description: "Anthropic Claude configuration template".into(),
            profile: template_profile(
                LlmProvider::Anthropic,
                "claude-4.5-opus",
                LlmGenerationParams {
                    temperature: Some(0.7),
                    max_tokens: Some(8192),
                    ..Default::default()
                },
            ),
        },
        LlmProfileTemplate {
            name: "gemini".into(),
            description: "Google Gemini configuration template".into(),
            profile: template_profile(
                LlmProvider::GeminiNative,
                "gemini-2.5-pro",
                LlmGenerationParams {
                    temperature: Some(0.7),
                    max_tokens: Some(8192),
                    ..Default::default()
                },
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
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

    fn profile(id: &str, provider: LlmProvider, model: &str, key: Option<&str>) -> LlmProfile {
        LlmProfile {
            id: id.into(),
            name: format!("profile {}", id),
            provider,
            model: model.into(),
            api_key: key.map(ToOwned::to_owned),
            ..template_profile(
                LlmProvider::OpenaiChat,
                "x",
                wf_types::llm::generation::LlmGenerationParams::default(),
            )
        }
    }

    #[tokio::test]
    async fn profile_crud_and_default() {
        let ctx = make_ctx();

        create(
            &ctx,
            &profile("p1", LlmProvider::OpenaiChat, "gpt-4o", Some("sk-1")),
        )
        .await
        .unwrap();
        create(
            &ctx,
            &profile("p2", LlmProvider::Anthropic, "claude-4", Some("sk-2")),
        )
        .await
        .unwrap();

        // First registered profile is the implicit default.
        assert_eq!(get_default_id(&ctx).await.unwrap().as_deref(), Some("p1"));

        // Duplicate id is rejected.
        let err = create(&ctx, &profile("p1", LlmProvider::OpenaiChat, "gpt-5", None))
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::AlreadyExists { .. }));

        // Update via the full-profile form.
        let mut updated = profile("p2", LlmProvider::Anthropic, "claude-5", Some("sk-3"));
        updated.max_retries = Some(3);
        update(&ctx, &updated).await.unwrap();
        assert_eq!(get(&ctx, "p2").await.unwrap().max_retries, Some(3));

        set_default(&ctx, "p2").await.unwrap();
        assert_eq!(get_default_id(&ctx).await.unwrap().as_deref(), Some("p2"));

        delete(&ctx, "p2").await.unwrap();
        // Fallback to the first remaining profile after removing the default.
        assert_eq!(get_default_id(&ctx).await.unwrap().as_deref(), Some("p1"));

        let err = delete(&ctx, "p2").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn export_masks_api_key_and_import_rejects_it() {
        let ctx = make_ctx();

        create(
            &ctx,
            &profile(
                "p-exp",
                LlmProvider::OpenaiChat,
                "gpt-4o",
                Some("sk-secret"),
            ),
        )
        .await
        .unwrap();

        let exported = export(&ctx, "p-exp").await.unwrap();
        assert_eq!(exported["api_key"], json!(MASKED_API_KEY));

        let json = export_json(&ctx, "p-exp").await.unwrap();
        assert!(json.contains(MASKED_API_KEY));
        assert!(!json.contains("sk-secret"));

        // Re-importing a masked export must fail.
        let err = import_json(&ctx, &json).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));

        // A clean import round-trips.
        let clean = serde_json::json!({
            "id": "p-imp",
            "name": "imported",
            "provider": "OPENAI_CHAT",
            "model": "gpt-4o",
            "api_key": "sk-imported",
        });
        let id = import_json(&ctx, &clean.to_string()).await.unwrap();
        assert_eq!(id, "p-imp");
        assert_eq!(
            get(&ctx, "p-imp").await.unwrap().api_key.as_deref(),
            Some("sk-imported")
        );
    }

    #[tokio::test]
    async fn templates_and_filter() {
        let ctx = make_ctx();

        let templates = list_templates(&ctx).await.unwrap();
        assert_eq!(templates.len(), 3);

        let id = create_from_template(
            &ctx,
            "openai-chat",
            &json!({ "id": "tpl-1", "api_key": "sk-tpl" }),
        )
        .await
        .unwrap();
        assert_eq!(id, "tpl-1");
        let created = get(&ctx, "tpl-1").await.unwrap();
        assert_eq!(created.model, "gpt-5");
        assert_eq!(created.provider, LlmProvider::OpenaiChat);

        // Generated id when none is supplied.
        let auto = create_from_template(&ctx, "gemini", &json!({ "api_key": "sk-g" }))
            .await
            .unwrap();
        assert!(auto.starts_with("profile-"));

        let matched = query(
            &ctx,
            &LlmProfileFilter {
                provider: Some(LlmProvider::GeminiNative),
                ..LlmProfileFilter::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, auto);
    }

    #[tokio::test]
    async fn custom_templates_add_remove() {
        let ctx = make_ctx();

        // Custom template registration is persisted per-context.
        let custom = LlmProfileTemplate {
            name: "custom-chat".into(),
            description: "A runtime template".into(),
            profile: template_profile(
                LlmProvider::OpenaiChat,
                "gpt-custom",
                wf_types::llm::generation::LlmGenerationParams::default(),
            ),
        };
        add_template(&ctx, custom.clone()).await.unwrap();
        let err = add_template(&ctx, custom.clone()).await.unwrap_err();
        assert!(matches!(err, ApiError::AlreadyExists { .. }));
        let err = add_template(
            &ctx,
            LlmProfileTemplate {
                name: "openai-chat".into(),
                description: "clashes with builtin".into(),
                profile: template_profile(
                    LlmProvider::OpenaiChat,
                    "x",
                    wf_types::llm::generation::LlmGenerationParams::default(),
                ),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::AlreadyExists { .. }));

        let templates = list_templates(&ctx).await.unwrap();
        assert_eq!(templates.len(), 4);
        assert!(get_template(&ctx, "custom-chat").await.unwrap().is_some());

        let id = create_from_template(&ctx, "custom-chat", &json!({ "api_key": "sk-c" }))
            .await
            .unwrap();
        assert!(id.starts_with("profile-"));

        assert!(remove_template(&ctx, "custom-chat").await.unwrap());
        assert!(!remove_template(&ctx, "custom-chat").await.unwrap());
        assert_eq!(list_templates(&ctx).await.unwrap().len(), 3);
    }
}

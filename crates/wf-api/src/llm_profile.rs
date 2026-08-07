use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use wf_llm::profile_manager::validate_profile;
use wf_types::llm::{LlmProfile, LlmProvider};

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// Mask placeholder used for exported API keys (mirrors the TS
/// `***HIDDEN***` convention so a masked export cannot round-trip a key).
pub const MASKED_API_KEY: &str = "***HIDDEN***";

/// One point of an LLM profile template (mirrors the TS
/// `LLMProfileTemplate`): a partial profile whose id/name/key are filled by
/// the caller at creation time.
#[derive(Debug, Clone, Serialize)]
pub struct LlmProfileTemplate {
    pub name: String,
    pub description: String,
    pub profile: LlmProfile,
}

/// Profile filter (mirrors the TS `LLMProfileFilter`).
#[derive(Debug, Clone, Default)]
pub struct LlmProfileFilter {
    pub id: Option<String>,
    pub name: Option<String>,
    pub provider: Option<LlmProvider>,
    pub model: Option<String>,
}

/// LLM profile resource management (TS `LLMProfileRegistryAPI`
/// counterpart).
///
/// Backed by the shared `LlmGateway` profile registry, so profiles managed
/// through this API are the same profiles every LLM request resolves.
pub struct LlmProfileApi {
    ctx: Arc<ApiContext>,
}

impl LlmProfileApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Register a profile; errors with `AlreadyExists` when the id is taken.
    pub async fn create(&self, profile: &LlmProfile) -> ApiResult<()> {
        let manager = self.ctx.llm_gateway.profile_registry();
        if manager.has(&profile.id) {
            return Err(ApiError::already_exists("profile", &profile.id));
        }
        validate_profile(profile)?;
        manager.register(profile.clone())?;
        Ok(())
    }

    /// Replace a profile with the given complete profile (must carry the
    /// target id). The idiomatic Rust form of the TS partial update: load,
    /// modify and pass the full record back.
    pub async fn update(&self, profile: &LlmProfile) -> ApiResult<()> {
        let manager = self.ctx.llm_gateway.profile_registry();
        if !manager.has(&profile.id) {
            return Err(ApiError::not_found("profile", &profile.id));
        }
        validate_profile(profile)?;
        manager.register(profile.clone())?;
        Ok(())
    }

    /// Get a profile by id; an empty id resolves to the default profile.
    pub async fn get(&self, id: &str) -> ApiResult<LlmProfile> {
        self.ctx
            .llm_gateway
            .profile_registry()
            .get(id)
            .ok_or_else(|| ApiError::not_found("profile", id))
    }

    /// List all registered profiles.
    pub async fn list(&self) -> ApiResult<Vec<LlmProfile>> {
        Ok(self.ctx.llm_gateway.profile_registry().list())
    }

    /// Delete a profile; errors with `NotFound` when it does not exist.
    pub async fn delete(&self, id: &str) -> ApiResult<()> {
        self.ctx
            .llm_gateway
            .remove_profile(id)
            .map(|_| ())
            .ok_or_else(|| ApiError::not_found("profile", id))
    }

    /// Filter the profile list by id/name/provider/model.
    pub async fn query(&self, filter: &LlmProfileFilter) -> ApiResult<Vec<LlmProfile>> {
        let profiles = self.list().await?;
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
    pub async fn set_default(&self, id: &str) -> ApiResult<()> {
        self.ctx.llm_gateway.profile_registry().set_default(id)?;
        Ok(())
    }

    pub async fn get_default(&self) -> ApiResult<Option<LlmProfile>> {
        Ok(self.ctx.llm_gateway.profile_registry().get_default())
    }

    pub async fn get_default_id(&self) -> ApiResult<Option<String>> {
        Ok(self.ctx.llm_gateway.profile_registry().default_id())
    }

    // ── export / import (key masking) ───────────────────────────────

    /// Export a profile with the API key masked. The masked value
    /// (`***HIDDEN***`) is rejected by [`Self::import_json`].
    pub async fn export(&self, id: &str) -> ApiResult<Value> {
        let profile = self.get(id).await?;
        Ok(mask_profile(&profile))
    }

    /// Export all profiles with API keys masked.
    pub async fn export_all(&self) -> ApiResult<Vec<Value>> {
        let profiles = self.list().await?;
        Ok(profiles.iter().map(mask_profile).collect())
    }

    /// Export one profile as a pretty JSON string (masked).
    pub async fn export_json(&self, id: &str) -> ApiResult<String> {
        let value = self.export(id).await?;
        Ok(serde_json::to_string_pretty(&value)?)
    }

    /// Export all profiles as a pretty JSON array (masked).
    pub async fn export_all_json(&self) -> ApiResult<String> {
        let value = self.export_all().await?;
        Ok(serde_json::to_string_pretty(&value)?)
    }

    /// Import one profile from a JSON string; rejects exports whose key is
    /// still masked. Returns the imported profile id.
    pub async fn import_json(&self, json: &str) -> ApiResult<String> {
        let profile: LlmProfile =
            serde_json::from_str(json).map_err(|e| ApiError::Validation(e.to_string()))?;
        if profile.api_key.as_deref() == Some(MASKED_API_KEY) {
            return Err(ApiError::Validation(
                "Cannot import a profile with a masked API key; supply the real key".into(),
            ));
        }
        self.create(&profile).await?;
        Ok(profile.id)
    }

    /// Import several profiles from a JSON array string; returns the ids of
    /// all successfully imported profiles.
    pub async fn import_all_json(&self, json: &str) -> ApiResult<Vec<String>> {
        let profiles: Vec<LlmProfile> =
            serde_json::from_str(json).map_err(|e| ApiError::Validation(e.to_string()))?;
        let mut ids = Vec::new();
        for profile in profiles {
            if profile.api_key.as_deref() == Some(MASKED_API_KEY) {
                continue;
            }
            if self.create(&profile).await.is_ok() {
                ids.push(profile.id);
            }
        }
        Ok(ids)
    }

    /// Validate a profile without registering it (mirrors the TS
    /// `validateProfile`).
    pub fn validate(&self, profile: &LlmProfile) -> (bool, Vec<String>) {
        let mut errors = Vec::new();
        if profile.id.trim().is_empty() {
            errors.push("Profile id is required".into());
        }
        if profile.name.trim().is_empty() {
            errors.push("Profile name is required".into());
        }
        if profile.model.trim().is_empty() {
            errors.push("Profile model is required".into());
        }
        if profile.api_key.as_deref().is_none_or(str::is_empty) {
            errors.push("Profile apiKey is required".into());
        }
        (errors.is_empty(), errors)
    }

    // ── templates ───────────────────────────────────────────────────

    /// List the built-in profile templates.
    pub async fn list_templates(&self) -> ApiResult<Vec<LlmProfileTemplate>> {
        Ok(builtin_templates())
    }

    pub async fn get_template(&self, name: &str) -> ApiResult<Option<LlmProfileTemplate>> {
        Ok(builtin_templates().into_iter().find(|t| t.name == name))
    }

    /// Create a profile from a template: start from the template profile and
    /// overlay the caller's overrides (JSON object of `LlmProfile` fields;
    /// `id`/`name` fall back to a generated id / the template name).
    pub async fn create_from_template(
        &self,
        template_name: &str,
        overrides: &Value,
    ) -> ApiResult<String> {
        let template = self
            .get_template(template_name)
            .await?
            .ok_or_else(|| ApiError::not_found("template", template_name))?;

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
        self.create(&profile).await?;
        Ok(profile.id)
    }
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

fn template_profile(provider: LlmProvider, model: &str, parameters: Value) -> LlmProfile {
    LlmProfile {
        id: String::new(),
        name: String::new(),
        provider,
        model: model.to_string(),
        api_key: None,
        base_url: None,
        parameters: Some(parameters),
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
    }
}

fn builtin_templates() -> Vec<LlmProfileTemplate> {
    vec![
        LlmProfileTemplate {
            name: "openai-chat".into(),
            description: "OpenAI Chat API configuration template".into(),
            profile: template_profile(
                LlmProvider::OpenaiChat,
                "gpt-5",
                json!({ "temperature": 0.7, "maxTokens": 8192 }),
            ),
        },
        LlmProfileTemplate {
            name: "anthropic".into(),
            description: "Anthropic Claude configuration template".into(),
            profile: template_profile(
                LlmProvider::Anthropic,
                "claude-4.5-opus",
                json!({ "temperature": 0.7, "maxTokens": 8192 }),
            ),
        },
        LlmProfileTemplate {
            name: "gemini".into(),
            description: "Google Gemini configuration template".into(),
            profile: template_profile(
                LlmProvider::GeminiNative,
                "gemini-2.5-pro",
                json!({ "temperature": 0.7, "maxOutputTokens": 8192 }),
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn profile(id: &str, provider: LlmProvider, model: &str, key: Option<&str>) -> LlmProfile {
        LlmProfile {
            id: id.into(),
            name: format!("profile {}", id),
            provider,
            model: model.into(),
            api_key: key.map(ToOwned::to_owned),
            ..template_profile(LlmProvider::OpenaiChat, "x", json!({}))
        }
    }

    #[tokio::test]
    async fn profile_crud_and_default() {
        let ctx = make_ctx();
        let api = LlmProfileApi::new(ctx);

        api.create(&profile(
            "p1",
            LlmProvider::OpenaiChat,
            "gpt-4o",
            Some("sk-1"),
        ))
        .await
        .unwrap();
        api.create(&profile(
            "p2",
            LlmProvider::Anthropic,
            "claude-4",
            Some("sk-2"),
        ))
        .await
        .unwrap();

        // First registered profile is the implicit default.
        assert_eq!(api.get_default_id().await.unwrap().as_deref(), Some("p1"));

        // Duplicate id is rejected.
        let err = api
            .create(&profile("p1", LlmProvider::OpenaiChat, "gpt-5", None))
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::AlreadyExists { .. }));

        // Update via the full-profile form.
        let mut updated = profile("p2", LlmProvider::Anthropic, "claude-5", Some("sk-3"));
        updated.max_retries = Some(3);
        api.update(&updated).await.unwrap();
        assert_eq!(api.get("p2").await.unwrap().max_retries, Some(3));

        api.set_default("p2").await.unwrap();
        assert_eq!(api.get_default_id().await.unwrap().as_deref(), Some("p2"));

        api.delete("p2").await.unwrap();
        // Fallback to the first remaining profile after removing the default.
        assert_eq!(api.get_default_id().await.unwrap().as_deref(), Some("p1"));

        let err = api.delete("p2").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn export_masks_api_key_and_import_rejects_it() {
        let ctx = make_ctx();
        let api = LlmProfileApi::new(ctx);

        api.create(&profile(
            "p-exp",
            LlmProvider::OpenaiChat,
            "gpt-4o",
            Some("sk-secret"),
        ))
        .await
        .unwrap();

        let exported = api.export("p-exp").await.unwrap();
        assert_eq!(exported["api_key"], json!(MASKED_API_KEY));

        let json = api.export_json("p-exp").await.unwrap();
        assert!(json.contains(MASKED_API_KEY));
        assert!(!json.contains("sk-secret"));

        // Re-importing a masked export must fail.
        let err = api.import_json(&json).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));

        // A clean import round-trips.
        let clean = serde_json::json!({
            "id": "p-imp",
            "name": "imported",
            "provider": "OPENAI_CHAT",
            "model": "gpt-4o",
            "api_key": "sk-imported",
        });
        let id = api.import_json(&clean.to_string()).await.unwrap();
        assert_eq!(id, "p-imp");
        assert_eq!(
            api.get("p-imp").await.unwrap().api_key.as_deref(),
            Some("sk-imported")
        );
    }

    #[tokio::test]
    async fn templates_and_filter() {
        let ctx = make_ctx();
        let api = LlmProfileApi::new(ctx);

        let templates = api.list_templates().await.unwrap();
        assert_eq!(templates.len(), 3);

        let id = api
            .create_from_template(
                "openai-chat",
                &json!({ "id": "tpl-1", "api_key": "sk-tpl" }),
            )
            .await
            .unwrap();
        assert_eq!(id, "tpl-1");
        let created = api.get("tpl-1").await.unwrap();
        assert_eq!(created.model, "gpt-5");
        assert_eq!(created.provider, LlmProvider::OpenaiChat);

        // Generated id when none is supplied.
        let auto = api
            .create_from_template("gemini", &json!({ "api_key": "sk-g" }))
            .await
            .unwrap();
        assert!(auto.starts_with("profile-"));

        let matched = api
            .query(&LlmProfileFilter {
                provider: Some(LlmProvider::GeminiNative),
                ..LlmProfileFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, auto);
    }
}

use std::sync::Arc;

use dashmap::DashMap;
use wf_types::llm::{LlmProfile, LlmProviderDefinition};

use crate::error::{LlmError, LlmResult};

/// Registry of declarative provider definitions (connection templates).
///
/// A definition carries the connection defaults shared by multiple profiles.
/// Profiles reference a definition through `LlmProfile::provider_id`; the
/// defaults are merged into the profile once at registration time, so the
/// request hot path never touches this registry.
#[derive(Clone, Default)]
pub struct ProviderDefinitionRegistry {
    providers: Arc<DashMap<String, LlmProviderDefinition>>,
}

impl ProviderDefinitionRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(DashMap::new()),
        }
    }

    /// Register a definition, replacing any previous entry under the same id.
    pub fn register(&self, definition: LlmProviderDefinition) -> LlmResult<()> {
        if definition.id.trim().is_empty() {
            return Err(LlmError::ConfigError(
                "Provider definition validation failed: 'id' is required".to_string(),
            ));
        }
        if definition.format.trim().is_empty() {
            return Err(LlmError::ConfigError(format!(
                "Provider definition '{}' is missing 'format'",
                definition.id
            )));
        }
        self.providers.insert(definition.id.clone(), definition);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<LlmProviderDefinition> {
        self.providers.get(id).map(|entry| entry.clone())
    }

    pub fn has(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    pub fn remove(&self, id: &str) -> Option<LlmProviderDefinition> {
        self.providers.remove(id).map(|(_, definition)| definition)
    }

    pub fn list(&self) -> Vec<LlmProviderDefinition> {
        self.providers.iter().map(|entry| entry.clone()).collect()
    }

    pub fn clear(&self) {
        self.providers.clear();
    }

    pub fn size(&self) -> usize {
        self.providers.len()
    }
}

/// Merge a provider definition's connection defaults into a profile.
///
/// Explicit profile fields always win; only missing fields are filled from
/// the definition. Fails when the referenced definition is unknown.
pub fn apply_provider_defaults(
    mut profile: LlmProfile,
    providers: &ProviderDefinitionRegistry,
) -> LlmResult<LlmProfile> {
    let Some(provider_id) = profile.provider_id.clone() else {
        return Ok(profile);
    };
    let definition = providers.get(&provider_id).ok_or_else(|| {
        LlmError::ConfigError(format!(
            "Profile '{}' references unknown provider '{}'",
            profile.id, provider_id
        ))
    })?;

    if profile.base_url.is_none() {
        profile.base_url = definition.base_url.clone();
    }
    if profile.auth_type.is_none() {
        profile.auth_type = definition.auth_type.clone();
    }
    if let Some(default_headers) = definition.default_headers.clone() {
        let custom_headers = profile.custom_headers.get_or_insert_with(Default::default);
        for (key, value) in default_headers {
            custom_headers.entry(key).or_insert(value);
        }
    }
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::LlmFormat;

    fn profile(id: &str) -> LlmProfile {
        LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            format: LlmFormat::OpenaiChat,
            provider_id: Some("acme".to_string()),
            model: "acme-7b".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        }
    }

    fn definition() -> LlmProviderDefinition {
        LlmProviderDefinition {
            id: "acme".to_string(),
            name: None,
            description: None,
            base_url: Some("https://api.acme.test".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: Some(
                [("x-tenant".to_string(), serde_json::json!("t1"))]
                    .into_iter()
                    .collect(),
            ),
            format: "OPENAI_CHAT".to_string(),
            model_discovery: None,
            api_version: None,
            metadata: None,
        }
    }

    #[test]
    fn register_rejects_blank_id_or_format() {
        let registry = ProviderDefinitionRegistry::new();
        let mut bad = definition();
        bad.id = "  ".to_string();
        assert!(registry.register(bad).is_err());
        let mut bad = definition();
        bad.format = String::new();
        assert!(registry.register(bad).is_err());
    }

    #[test]
    fn defaults_fill_only_missing_fields() {
        let registry = ProviderDefinitionRegistry::new();
        registry.register(definition()).unwrap();

        let mut profile = profile("p1");
        profile.base_url = Some("https://override.test".to_string());
        let merged = apply_provider_defaults(profile, &registry).unwrap();
        assert_eq!(
            merged.base_url.as_deref(),
            Some("https://override.test"),
            "explicit profile values win"
        );
        assert_eq!(merged.auth_type.as_deref(), Some("bearer"));
        assert_eq!(
            merged.custom_headers.as_ref().unwrap().get("x-tenant"),
            Some(&serde_json::json!("t1"))
        );
    }

    #[test]
    fn unknown_provider_id_fails() {
        let registry = ProviderDefinitionRegistry::new();
        assert!(apply_provider_defaults(profile("p1"), &registry).is_err());
    }

    #[test]
    fn profile_without_provider_id_passes_through() {
        let registry = ProviderDefinitionRegistry::new();
        let mut profile = profile("p1");
        profile.provider_id = None;
        let merged = apply_provider_defaults(profile.clone(), &registry).unwrap();
        assert_eq!(merged, profile);
    }
}

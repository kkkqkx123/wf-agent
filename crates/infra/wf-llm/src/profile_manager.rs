use crate::error::{LlmError, LlmResult};
use dashmap::DashMap;
use std::sync::{Arc, Mutex};
use wf_types::llm::LlmProfile;

/// Manages LLM profiles with default-profile semantics (first registered
/// profile becomes the default; an explicit default can be set; removing the
/// default falls back to the first remaining profile).
#[derive(Clone)]
pub struct ProfileManager {
    profiles: Arc<DashMap<String, LlmProfile>>,
    default_id: Arc<Mutex<Option<String>>>,
}

impl ProfileManager {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(DashMap::new()),
            default_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Register a profile, validating required fields (id/name/model) early
    /// so misconfiguration surfaces at assembly time. The first registered
    /// profile becomes the default.
    pub fn register(&self, profile: LlmProfile) -> LlmResult<()> {
        validate_profile(&profile)?;
        let is_first = self.profiles.is_empty();
        self.profiles.insert(profile.id.clone(), profile);
        if is_first {
            *wf_common::lock::lock_ok(self.default_id.lock()) = Some(
                self.profiles
                    .iter()
                    .next()
                    .map(|r| r.key().clone())
                    .unwrap_or_default(),
            );
        }
        Ok(())
    }

    /// Resolve a profile by id; an empty id resolves to the default profile.
    pub fn get(&self, id: &str) -> Option<LlmProfile> {
        if id.is_empty() {
            return self.get_default();
        }
        self.profiles.get(id).map(|r| r.clone())
    }

    /// Get the default profile.
    pub fn get_default(&self) -> Option<LlmProfile> {
        let id = wf_common::lock::lock_ok(self.default_id.lock()).clone()?;
        self.profiles.get(&id).map(|r| r.clone())
    }

    /// Set the default profile.
    pub fn set_default(&self, id: &str) -> LlmResult<()> {
        if !self.profiles.contains_key(id) {
            return Err(LlmError::ProfileNotFound(id.to_string()));
        }
        *wf_common::lock::lock_ok(self.default_id.lock()) = Some(id.to_string());
        Ok(())
    }

    pub fn list(&self) -> Vec<LlmProfile> {
        self.profiles.iter().map(|r| r.clone()).collect()
    }

    /// Remove a profile; if it was the default, fall back to the first
    /// remaining profile (or none).
    pub fn remove(&self, id: &str) -> Option<LlmProfile> {
        let removed = self.profiles.remove(id).map(|(_, v)| v)?;
        let mut default_id = wf_common::lock::lock_ok(self.default_id.lock());
        if default_id.as_deref() == Some(id) {
            *default_id = self.profiles.iter().next().map(|r| r.key().clone());
        }
        Some(removed)
    }

    pub fn clear(&self) {
        self.profiles.clear();
        *wf_common::lock::lock_ok(self.default_id.lock()) = None;
    }

    pub fn has(&self, id: &str) -> bool {
        self.profiles.contains_key(id)
    }

    pub fn size(&self) -> usize {
        self.profiles.len()
    }

    pub fn default_id(&self) -> Option<String> {
        wf_common::lock::lock_ok(self.default_id.lock()).clone()
    }
}

/// Validate a profile's required fields and provider-specific generation
/// constraints (`api_key` is optional because it may be injected per
/// request).
pub fn validate_profile(profile: &LlmProfile) -> LlmResult<()> {
    if profile.id.trim().is_empty() {
        return Err(LlmError::ConfigError(
            "Profile validation failed: 'id' is required".to_string(),
        ));
    }
    if profile.name.trim().is_empty() {
        return Err(LlmError::ConfigError(format!(
            "Profile validation failed: profile '{}' is missing 'name'",
            profile.id
        )));
    }
    if profile.model.trim().is_empty() {
        return Err(LlmError::ConfigError(format!(
            "Profile validation failed: profile '{}' is missing 'model'",
            profile.id
        )));
    }
    if let Some(ref gen) = profile.generation {
        validate_generation_params(gen, &profile.provider, &profile.id)?;
    }
    Ok(())
}

/// Validate provider-specific generation parameter constraints at profile
/// registration time. Constraints that depend on per-request values (e.g.
/// `budget_tokens < max_tokens`) are deferred to runtime validation.
fn validate_generation_params(
    gen: &wf_types::llm::generation::LlmGenerationParams,
    provider: &wf_types::llm::LlmProvider,
    profile_id: &str,
) -> LlmResult<()> {
    if let Some(ref thinking) = gen.thinking {
        if matches!(provider, wf_types::llm::LlmProvider::GeminiNative)
            && thinking.level.is_some()
            && thinking.budget_tokens.is_some()
        {
            return Err(LlmError::ConfigError(format!(
                "Profile '{}': Gemini thinkingLevel and thinkingBudget are mutually exclusive; set only one",
                profile_id
            )));
        }
        if matches!(provider, wf_types::llm::LlmProvider::Anthropic) {
            if let Some(budget) = thinking.budget_tokens {
                if budget < 1024 {
                    return Err(LlmError::ConfigError(format!(
                        "Profile '{}': Anthropic thinking budget_tokens must be at least 1024, got {}",
                        profile_id, budget
                    )));
                }
            }
        }
    }
    Ok(())
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::LlmProvider;

    fn valid_profile() -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            provider: LlmProvider::OpenaiChat,
            model: "gpt-4o".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            generation: None,
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

    fn profile_with_id(id: &str) -> LlmProfile {
        let mut p = valid_profile();
        p.id = id.to_string();
        p
    }

    #[test]
    fn register_validates_required_fields() {
        let manager = ProfileManager::new();

        let mut no_id = valid_profile();
        no_id.id = String::new();
        assert!(manager.register(no_id).is_err());

        let mut no_model = valid_profile();
        no_model.model = "   ".to_string();
        assert!(manager.register(no_model).is_err());

        assert!(manager.register(valid_profile()).is_ok());
        assert!(manager.get("p1").is_some());
    }

    #[test]
    fn first_registered_becomes_default_and_empty_get_resolves_to_default() {
        let manager = ProfileManager::new();
        manager.register(profile_with_id("p1")).unwrap();
        manager.register(profile_with_id("p2")).unwrap();

        assert_eq!(manager.get_default().unwrap().id, "p1");
        assert_eq!(manager.get("").unwrap().id, "p1");
        assert_eq!(manager.default_id().as_deref(), Some("p1"));
    }

    #[test]
    fn set_default_and_remove_fallback() {
        let manager = ProfileManager::new();
        manager.register(profile_with_id("p1")).unwrap();
        manager.register(profile_with_id("p2")).unwrap();

        manager.set_default("p2").unwrap();
        assert_eq!(manager.get_default().unwrap().id, "p2");
        assert!(manager.set_default("nope").is_err());

        manager.remove("p2");
        assert_eq!(manager.get_default().unwrap().id, "p1");

        manager.remove("p1");
        assert!(manager.get_default().is_none());
        assert!(manager.get("").is_none());
    }

    #[test]
    fn clear_and_has_and_size() {
        let manager = ProfileManager::new();
        manager.register(profile_with_id("p1")).unwrap();
        assert!(manager.has("p1"));
        assert_eq!(manager.size(), 1);

        manager.clear();
        assert_eq!(manager.size(), 0);
        assert!(!manager.has("p1"));
        assert!(manager.get_default().is_none());
    }

    #[test]
    fn gemini_thinking_level_and_budget_mutually_exclusive() {
        use wf_types::llm::generation::{LlmGenerationParams, LlmThinkingConfig, ThinkingLevel};

        let mut profile = valid_profile();
        profile.provider = LlmProvider::GeminiNative;
        profile.generation = Some(LlmGenerationParams {
            thinking: Some(LlmThinkingConfig {
                level: Some(ThinkingLevel::High),
                budget_tokens: Some(1024),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(validate_profile(&profile).is_err());
    }

    #[test]
    fn gemini_thinking_level_only_accepted() {
        use wf_types::llm::generation::{LlmGenerationParams, LlmThinkingConfig, ThinkingLevel};

        let mut profile = valid_profile();
        profile.provider = LlmProvider::GeminiNative;
        profile.generation = Some(LlmGenerationParams {
            thinking: Some(LlmThinkingConfig {
                level: Some(ThinkingLevel::High),
                budget_tokens: None,
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(validate_profile(&profile).is_ok());
    }

    #[test]
    fn anthropic_budget_tokens_below_minimum_rejected() {
        use wf_types::llm::generation::{LlmGenerationParams, LlmThinkingConfig};

        let mut profile = valid_profile();
        profile.provider = LlmProvider::Anthropic;
        profile.generation = Some(LlmGenerationParams {
            thinking: Some(LlmThinkingConfig {
                level: None,
                budget_tokens: Some(512),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(validate_profile(&profile).is_err());
    }

    #[test]
    fn anthropic_budget_tokens_above_minimum_accepted() {
        use wf_types::llm::generation::{LlmGenerationParams, LlmThinkingConfig};

        let mut profile = valid_profile();
        profile.provider = LlmProvider::Anthropic;
        profile.generation = Some(LlmGenerationParams {
            thinking: Some(LlmThinkingConfig {
                level: None,
                budget_tokens: Some(2048),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(validate_profile(&profile).is_ok());
    }
}

use crate::error::{LlmError, LlmResult};
use dashmap::DashMap;
use std::sync::Arc;
use wf_types::llm::LlmProfile;

#[derive(Clone)]
pub struct ProfileManager {
    profiles: Arc<DashMap<String, LlmProfile>>,
}

impl ProfileManager {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(DashMap::new()),
        }
    }

    /// Register a profile, validating required fields (id/name/provider/model)
    /// early so misconfiguration surfaces at assembly time.
    pub fn register(&self, profile: LlmProfile) -> LlmResult<()> {
        validate_profile(&profile)?;
        self.profiles.insert(profile.id.clone(), profile);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<LlmProfile> {
        self.profiles.get(id).map(|r| r.clone())
    }

    pub fn list(&self) -> Vec<LlmProfile> {
        self.profiles.iter().map(|r| r.clone()).collect()
    }

    pub fn remove(&self, id: &str) -> Option<LlmProfile> {
        self.profiles.remove(id).map(|(_, v)| v)
    }
}

/// Validate a profile's required fields (mirrors the TS `validateProfile`
/// semantics; `api_key` is optional because it may be injected per request).
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
}

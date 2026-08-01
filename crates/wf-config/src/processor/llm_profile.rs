use std::collections::HashMap;

use crate::error::ConfigResult;
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_required;

use wf_types::llm::profile::LlmProfile;

pub fn validate_llm_profile(profile: &LlmProfile) -> ConfigResult<()> {
    validate_required(&profile.id, "id")?;
    validate_required(&profile.name, "name")?;
    validate_required(&profile.model, "model")?;
    Ok(())
}

pub fn transform_llm_profile(
    profile: &LlmProfile,
    parameters: &HashMap<String, String>,
) -> ConfigResult<LlmProfile> {
    let mut cloned = profile.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_llm_profile(profile: LlmProfile) -> LlmProfile {
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile() -> LlmProfile {
        LlmProfile {
            id: "test-id".to_string(),
            name: "test".to_string(),
            provider: wf_types::llm::LlmProvider::OpenaiChat,
            model: "gpt-4".to_string(),
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
    fn test_valid_profile() {
        let profile = make_profile();
        assert!(validate_llm_profile(&profile).is_ok());
    }

    #[test]
    fn test_empty_id() {
        let mut profile = make_profile();
        profile.id = String::new();
        assert!(validate_llm_profile(&profile).is_err());
    }

    #[test]
    fn test_empty_model() {
        let mut profile = make_profile();
        profile.model = String::new();
        assert!(validate_llm_profile(&profile).is_err());
    }

    #[test]
    fn test_transform_llm_profile() {
        let profile = make_profile();
        let mut params = HashMap::new();
        params.insert("version".to_string(), "v2".to_string());

        let result = transform_llm_profile(&profile, &params).unwrap();
        assert_eq!(result.id, "test-id");
        assert_eq!(result.model, "gpt-4");
    }

    #[test]
    fn test_export_llm_profile() {
        let profile = make_profile();
        let exported = export_llm_profile(profile.clone());
        assert_eq!(exported.id, profile.id);
        assert_eq!(exported.model, profile.model);
    }
}

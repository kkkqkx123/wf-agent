use std::collections::HashMap;

use crate::error::ConfigResult;
use crate::processor::substitute::substitute_in_struct;
use crate::validator::validate_not_empty;

use wf_types::llm::profile::LlmProfile;
use wf_types::llm::provider_definition::LlmProviderDefinition;

pub fn validate_llm_profile(profile: &LlmProfile) -> ConfigResult<()> {
    validate_not_empty(&profile.id, "id")?;
    validate_not_empty(&profile.name, "name")?;
    validate_not_empty(&profile.model, "model")?;
    if let Some(proxy) = profile.proxy.as_deref() {
        if has_proxy_userinfo(proxy) {
            return Err(crate::error::ConfigError::Validation(
                "proxy must be route-only; store credentials as a proxy credential instead"
                    .to_string(),
            ));
        }
    }
    reject_command_secret("api_key", profile.api_key.as_deref())?;
    reject_command_header_values(profile.headers.as_ref())?;
    reject_command_header_values(profile.custom_headers.as_ref())?;
    if let Err(reason) = wf_types::llm::parse_request_limits(&profile.parameters) {
        return Err(crate::error::ConfigError::Validation(format!(
            "invalid request limits: {reason}"
        )));
    }
    Ok(())
}

/// Shell-command secret expansion (`!command`, Pi `models.json` style) is
/// intentionally unsupported: per-request process spawning adds an
/// injection surface, latency jitter and unaudited credential reads. A
/// leading `!` therefore fails fast with a pointer to `${VAR}`
/// interpolation or plugin auth instead of being sent as a literal key.
fn reject_command_secret(field: &str, value: Option<&str>) -> ConfigResult<()> {
    if value.is_some_and(|v| v.starts_with('!')) {
        return Err(crate::error::ConfigError::Validation(format!(
            "'{field}' starts with '!': shell-command secrets are not executed; use ${{VAR}} interpolation or plugin auth instead"
        )));
    }
    Ok(())
}

fn reject_command_header_values(
    headers: Option<&std::collections::HashMap<String, serde_json::Value>>,
) -> ConfigResult<()> {
    if let Some(headers) = headers {
        for (name, value) in headers {
            if value.as_str().is_some_and(|v| v.starts_with('!')) {
                return Err(crate::error::ConfigError::Validation(format!(
                    "header '{name}' starts with '!': shell-command secrets are not executed; use ${{VAR}} interpolation or plugin auth instead"
                )));
            }
        }
    }
    Ok(())
}

/// Route-only guard mirroring the shared proxy helper, kept inline so this
/// network-free config crate gains no transport dependency.
fn has_proxy_userinfo(url: &str) -> bool {
    let Some((_, rest)) = url.trim().split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority.contains('@')
}

/// Validate a provider definition's required fields at load time.
pub fn validate_provider_definition(definition: &LlmProviderDefinition) -> ConfigResult<()> {
    validate_not_empty(&definition.id, "id")?;
    if let wf_types::llm::LlmFormat::Custom(name) = &definition.format {
        validate_not_empty(name, "format")?;
    }
    if let Some(proxy) = definition.proxy.as_deref() {
        if has_proxy_userinfo(proxy) {
            return Err(crate::error::ConfigError::Validation(
                "proxy must be route-only; store credentials as a proxy credential instead"
                    .to_string(),
            ));
        }
    }
    reject_command_header_values(definition.default_headers.as_ref())?;
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
            format: wf_types::llm::LlmFormat::OpenaiChat,
            provider_id: None,
            model: "gpt-4".to_string(),
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
            max_output_tokens: None,
            proxy: None,
            no_proxy: None,
            circuit_breaker: None,
            prompt_caching: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
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
    fn test_whitespace_only_id_rejected() {
        let mut profile = make_profile();
        profile.id = "   ".to_string();
        assert!(validate_llm_profile(&profile).is_err());
    }

    #[test]
    fn test_whitespace_only_name_rejected() {
        let mut profile = make_profile();
        profile.name = "   ".to_string();
        assert!(validate_llm_profile(&profile).is_err());
    }

    #[test]
    fn test_whitespace_only_model_rejected() {
        let mut profile = make_profile();
        profile.model = "   ".to_string();
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

    #[test]
    fn test_command_secret_prefix_rejected() {
        let mut profile = make_profile();
        profile.api_key = Some("!vault read secret".to_string());
        assert!(validate_llm_profile(&profile).is_err());

        let mut headed = make_profile();
        headed.custom_headers = Some(
            [("x-key".to_string(), serde_json::json!("!vault read secret"))]
                .into_iter()
                .collect(),
        );
        assert!(validate_llm_profile(&headed).is_err());
    }

    #[test]
    fn test_mistyped_request_limits_rejected() {
        let mut profile = make_profile();
        profile.parameters = Some(serde_json::json!({
            "request_limits": {"max_images_per_request": "many"}
        }));
        assert!(validate_llm_profile(&profile).is_err());
    }
}

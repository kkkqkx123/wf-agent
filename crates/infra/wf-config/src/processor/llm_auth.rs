//! Validation and transformation for declarative LLM auth configuration.
//!
//! Declarations carry no secrets: endpoints, client ids and environment
//! variable names only. Validation is fail-fast at load time so a
//! mis-declared provider surfaces as a config error, not a login failure.

use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_not_empty;

use wf_types::config::llm_auth::{AuthMethodConfig, AuthProviderConfig, OAuthFlowConfig};

fn missing(provider_id: &str, field: &str, requirement: &str) -> ConfigError {
    ConfigError::Validation(format!(
        "auth provider '{provider_id}': {requirement} requires {field}"
    ))
}

/// Validate one declared provider auth surface.
pub fn validate_auth_provider(provider: &AuthProviderConfig) -> ConfigResult<()> {
    validate_not_empty(&provider.id, "id")?;
    validate_not_empty(&provider.display_name, "display_name")?;
    match provider.method {
        AuthMethodConfig::ApiKey => {
            let env = provider
                .env
                .as_ref()
                .ok_or_else(|| missing(&provider.id, "env", "method \"api_key\""))?;
            validate_not_empty(env, "env")?;
        }
        AuthMethodConfig::OAuth => {
            let flow = provider
                .flow
                .ok_or_else(|| missing(&provider.id, "flow", "method \"oauth\""))?;
            let token_url = provider
                .token_url
                .as_ref()
                .ok_or_else(|| missing(&provider.id, "token_url", "method \"oauth\""))?;
            validate_not_empty(token_url, "token_url")?;
            let client_id = provider
                .client_id
                .as_ref()
                .ok_or_else(|| missing(&provider.id, "client_id", "method \"oauth\""))?;
            validate_not_empty(client_id, "client_id")?;
            match flow {
                OAuthFlowConfig::Device => {
                    let url = provider.device_authorization_url.as_ref().ok_or_else(|| {
                        missing(&provider.id, "device_authorization_url", "flow \"device\"")
                    })?;
                    validate_not_empty(url, "device_authorization_url")?;
                }
                OAuthFlowConfig::Pkce => {
                    let authorize = provider
                        .authorize_url
                        .as_ref()
                        .ok_or_else(|| missing(&provider.id, "authorize_url", "flow \"pkce\""))?;
                    validate_not_empty(authorize, "authorize_url")?;
                    let redirect = provider
                        .redirect_uri
                        .as_ref()
                        .ok_or_else(|| missing(&provider.id, "redirect_uri", "flow \"pkce\""))?;
                    validate_not_empty(redirect, "redirect_uri")?;
                }
            }
        }
    }
    Ok(())
}

/// Validate a whole auth configuration: every declared surface plus
/// duplicate-id detection (a duplicate would silently shadow the first).
pub fn validate_auth_config(providers: &[AuthProviderConfig]) -> ConfigResult<()> {
    let mut seen = std::collections::HashSet::new();
    for provider in providers {
        validate_auth_provider(provider)?;
        if !seen.insert(provider.id.clone()) {
            return Err(ConfigError::Validation(format!(
                "duplicate auth provider id '{}'",
                provider.id
            )));
        }
    }
    Ok(())
}

/// Substitute `${VAR}` references in every declared field. Endpoints and
/// client ids may come from the environment; secrets never appear here.
pub fn transform_auth_providers(
    providers: &[AuthProviderConfig],
    parameters: &std::collections::HashMap<String, String>,
) -> ConfigResult<Vec<AuthProviderConfig>> {
    let mut cloned = providers.to_vec();
    for provider in &mut cloned {
        let mut value = serde_json::to_value(&*provider).map_err(|e| {
            ConfigError::Serialization(format!("failed to serialize auth provider: {e}"))
        })?;
        substitute_auth_value(&mut value, parameters);
        *provider = serde_json::from_value(value).map_err(|e| {
            ConfigError::Validation(format!(
                "substituted auth provider no longer matches its declared shape: {e}"
            ))
        })?;
    }
    Ok(cloned)
}

fn substitute_auth_value(
    value: &mut serde_json::Value,
    parameters: &std::collections::HashMap<String, String>,
) {
    match value {
        serde_json::Value::String(s) => {
            *s = substitute_placeholders(s, parameters);
        }
        serde_json::Value::Array(items) => {
            for item in items {
                substitute_auth_value(item, parameters);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                substitute_auth_value(v, parameters);
            }
        }
        _ => {}
    }
}

/// Expand `${VAR}` / `${VAR:default}` / `${VAR:-default}` in one string.
///
/// Resolution order is explicit `parameters` first, then the process
/// environment, then the inline default. Placeholders whose name is not a
/// plain env identifier (for example workflow `${input.a}`) are left
/// untouched so declaration interpolation never collides with other
/// expression syntaxes.
fn substitute_placeholders(
    input: &str,
    parameters: &std::collections::HashMap<String, String>,
) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = input[i + 2..].find('}') {
                let inner = &input[i + 2..i + 2 + end];
                if let Some((name, default)) = split_placeholder(inner) {
                    if is_env_name(name) {
                        let replacement = parameters
                            .get(name)
                            .cloned()
                            .or_else(|| {
                                std::env::var(name).ok().filter(|v| !v.is_empty())
                            })
                            .or_else(|| default.map(str::to_string));
                        if let Some(replacement) = replacement {
                            out.push_str(&replacement);
                            i += 2 + end + 1;
                            continue;
                        }
                    }
                }
                out.push_str(&input[i..i + 2 + end + 1]);
                i += 2 + end + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn split_placeholder(inner: &str) -> Option<(&str, Option<&str>)> {
    if inner.is_empty() {
        return None;
    }
    if let Some(pos) = inner.find(":-") {
        let (name, default) = inner.split_at(pos);
        return Some((name, Some(&default[2..])));
    }
    if let Some(pos) = inner.find(':') {
        let (name, default) = inner.split_at(pos);
        return Some((name, Some(&default[1..])));
    }
    Some((inner, None))
}

fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::llm_auth::AuthMethodConfig;

    fn api_key_provider() -> AuthProviderConfig {
        AuthProviderConfig {
            id: "acme".to_string(),
            method: AuthMethodConfig::ApiKey,
            display_name: "Acme API key".to_string(),
            login_label: None,
            env: Some("ACME_API_KEY".to_string()),
            flow: None,
            is_subscription: None,
            device_authorization_url: None,
            authorize_url: None,
            token_url: None,
            client_id: None,
            scope: None,
            redirect_uri: None,
            loopback: None,
        }
    }

    fn device_provider() -> AuthProviderConfig {
        AuthProviderConfig {
            id: "acme-sub".to_string(),
            method: AuthMethodConfig::OAuth,
            display_name: "Acme (Pro)".to_string(),
            login_label: None,
            env: None,
            flow: Some(OAuthFlowConfig::Device),
            is_subscription: Some(true),
            device_authorization_url: Some("https://acme.test/device".to_string()),
            authorize_url: None,
            token_url: Some("https://acme.test/token".to_string()),
            client_id: Some("cid".to_string()),
            scope: None,
            redirect_uri: None,
            loopback: None,
        }
    }

    #[test]
    fn valid_declarations_pass() {
        assert!(validate_auth_config(&[api_key_provider(), device_provider()]).is_ok());
    }

    #[test]
    fn api_key_without_env_is_rejected() {
        let mut provider = api_key_provider();
        provider.env = None;
        let err = validate_auth_provider(&provider).expect_err("must fail");
        assert!(err.to_string().contains("env"), "{err}");
    }

    #[test]
    fn oauth_without_flow_is_rejected() {
        let mut provider = device_provider();
        provider.flow = None;
        let err = validate_auth_provider(&provider).expect_err("must fail");
        assert!(err.to_string().contains("flow"), "{err}");
    }

    #[test]
    fn device_flow_without_device_url_is_rejected() {
        let mut provider = device_provider();
        provider.device_authorization_url = None;
        let err = validate_auth_provider(&provider).expect_err("must fail");
        assert!(
            err.to_string().contains("device_authorization_url"),
            "{err}"
        );
    }

    #[test]
    fn pkce_flow_requires_authorize_and_redirect() {
        let mut provider = device_provider();
        provider.flow = Some(OAuthFlowConfig::Pkce);
        let err = validate_auth_provider(&provider).expect_err("must fail");
        assert!(err.to_string().contains("authorize_url"), "{err}");

        provider.authorize_url = Some("https://acme.test/authorize".to_string());
        let err = validate_auth_provider(&provider).expect_err("must fail");
        assert!(err.to_string().contains("redirect_uri"), "{err}");

        provider.redirect_uri = Some("http://127.0.0.1:8080/callback".to_string());
        assert!(validate_auth_provider(&provider).is_ok());
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let err =
            validate_auth_config(&[api_key_provider(), api_key_provider()]).expect_err("must fail");
        assert!(err.to_string().contains("duplicate"), "{err}");
    }

    #[test]
    fn transform_substitutes_environment_references() {
        let mut provider = device_provider();
        provider.token_url = Some("https://${TOKEN_HOST}/token".to_string());
        let mut params = std::collections::HashMap::new();
        params.insert("TOKEN_HOST".to_string(), "acme.test".to_string());
        let transformed = transform_auth_providers(&[provider], &params).expect("transform");
        assert_eq!(
            transformed[0].token_url.as_deref(),
            Some("https://acme.test/token")
        );
    }

    #[test]
    fn transform_reads_process_environment_and_defaults() {
        std::env::set_var("WF_TEST_TOKEN_HOST", "env-host.test");
        let mut provider = device_provider();
        provider.token_url = Some("https://${WF_TEST_TOKEN_HOST}/token".to_string());
        let transformed = transform_auth_providers(
            &[provider],
            &std::collections::HashMap::new(),
        )
        .expect("transform");
        assert_eq!(
            transformed[0].token_url.as_deref(),
            Some("https://env-host.test/token")
        );
        std::env::remove_var("WF_TEST_TOKEN_HOST");

        let mut provider = device_provider();
        provider.token_url = Some("https://${WF_TEST_MISSING_HOST:fallback.test}/token".to_string());
        std::env::remove_var("WF_TEST_MISSING_HOST");
        let transformed = transform_auth_providers(
            &[provider],
            &std::collections::HashMap::new(),
        )
        .expect("transform");
        assert_eq!(
            transformed[0].token_url.as_deref(),
            Some("https://fallback.test/token")
        );
    }

    #[test]
    fn transform_leaves_workflow_expressions_untouched() {
        let mut provider = device_provider();
        provider.token_url = Some("https://${input.a}/token".to_string());
        let transformed = transform_auth_providers(
            &[provider],
            &std::collections::HashMap::new(),
        )
        .expect("transform");
        assert_eq!(
            transformed[0].token_url.as_deref(),
            Some("https://${input.a}/token")
        );
    }
}

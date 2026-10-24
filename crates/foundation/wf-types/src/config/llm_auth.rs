//! Declarative LLM authentication configuration.
//!
//! Hosts declare how each provider authenticates (ambient environment key,
//! device-flow OAuth, PKCE OAuth) without any secret in the file: tokens
//! live in the credential store, endpoints and client ids live here. The
//! declaration drives the built-in auth surfaces
//! (`wf_llm::auth::{EnvApiKeyAuth, DeclaredOAuth}`); plugins may register
//! richer implementations under the same provider id.

use serde::{Deserialize, Serialize};

/// Which authentication method a declared provider uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethodConfig {
    /// Static API key from the credential store or an environment variable.
    #[default]
    ApiKey,
    /// OAuth login with a refresh lifecycle.
    #[serde(rename = "oauth")]
    OAuth,
}

/// Which OAuth flow a declared provider supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthFlowConfig {
    /// Device authorization grant (RFC 8628).
    Device,
    /// Authorization code with PKCE (RFC 7636).
    Pkce,
}

/// One declared provider authentication surface. All fields except `id`,
/// `method` and `display_name` are method-specific.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthProviderConfig {
    /// Provider id keying the credential store and matching
    /// `LlmProviderDefinition::id`.
    pub id: String,
    /// Authentication method.
    #[serde(default)]
    pub method: AuthMethodConfig,
    /// Display name, e.g. "Anthropic API key".
    pub display_name: String,
    /// Selector label for the login option, e.g. "Sign in with Claude Pro/Max".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_label: Option<String>,

    // --- api_key method ---
    /// Environment variable consulted when nothing is stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,

    // --- oauth method ---
    /// Which flow the provider supports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<OAuthFlowConfig>,
    /// True when access through this flow is subscription-backed; hosts
    /// suppress per-token cost accounting for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_subscription: Option<bool>,
    /// Device authorization endpoint (`flow = device`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_authorization_url: Option<String>,
    /// Authorization endpoint (`flow = pkce`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorize_url: Option<String>,
    /// Token endpoint (both flows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    /// OAuth client id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Requested scopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Redirect URI (`flow = pkce`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    /// Bind a loopback listener instead of asking for a pasted callback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loopback: Option<bool>,
}

/// LLM authentication configuration: the credential store location plus
/// the declared provider surfaces. Default is empty, which keeps the
/// static-key behavior unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Credential store path. `None` uses the host default
    /// (`~/.wf/auth.json`). Relative paths resolve against the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_path: Option<String>,
    /// Declared provider authentication surfaces.
    #[serde(default)]
    pub providers: Vec<AuthProviderConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> AuthProviderConfig {
        AuthProviderConfig {
            id: "acme-sub".to_string(),
            method: AuthMethodConfig::OAuth,
            display_name: "Acme (Pro)".to_string(),
            login_label: Some("Sign in with Acme Pro".to_string()),
            env: None,
            flow: Some(OAuthFlowConfig::Device),
            is_subscription: Some(true),
            device_authorization_url: Some("https://acme.test/device".to_string()),
            authorize_url: None,
            token_url: Some("https://acme.test/token".to_string()),
            client_id: Some("cid".to_string()),
            scope: Some("api".to_string()),
            redirect_uri: None,
            loopback: None,
        }
    }

    #[test]
    fn default_config_is_empty() {
        let config = AuthConfig::default();
        assert!(config.providers.is_empty());
        assert!(config.store_path.is_none());
    }

    #[test]
    fn provider_roundtrips_through_toml() {
        let toml = toml::to_string(&provider()).expect("serialize");
        assert!(toml.contains("method = \"oauth\""));
        assert!(toml.contains("flow = \"device\""));
        let back: AuthProviderConfig = toml::from_str(&toml).expect("deserialize");
        assert_eq!(back, provider());
    }

    #[test]
    fn api_key_provider_needs_only_id_and_env() {
        let toml = r#"
id = "acme"
method = "api_key"
display_name = "Acme API key"
env = "ACME_API_KEY"
"#;
        let parsed: AuthProviderConfig = toml::from_str(toml).expect("deserialize");
        assert_eq!(parsed.method, AuthMethodConfig::ApiKey);
        assert_eq!(parsed.env.as_deref(), Some("ACME_API_KEY"));
        assert!(parsed.flow.is_none());
    }
}

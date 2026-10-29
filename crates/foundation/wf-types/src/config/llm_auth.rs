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
/// `method` and `display_name` are method-specific. When `preset` names a
/// built-in OAuth preset (e.g. `"openai-codex"`), missing endpoints are
/// filled from the preset; explicit fields always win.
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
    /// Built-in OAuth preset name (e.g. `"openai-codex"`,
    /// `"github-copilot"`, `"google-vertex"`). When set, missing OAuth
    /// endpoints are filled from the preset before validation; explicit
    /// fields always win over the preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
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

/// All built-in OAuth preset names in stable order.
pub fn oauth_preset_names() -> Vec<&'static str> {
    vec!["openai-codex", "github-copilot", "google-vertex"]
}

/// Template for a built-in OAuth preset. The template carries the
/// well-known endpoints, flow and scope; `id`, `display_name` and
/// `login_label` stay with the caller. Returns `None` for unknown names.
pub fn oauth_preset_template(name: &str) -> Option<AuthProviderConfig> {
    match name {
        "openai-codex" => Some(AuthProviderConfig {
            id: String::new(),
            method: AuthMethodConfig::OAuth,
            display_name: String::new(),
            login_label: Some("Sign in with ChatGPT".to_string()),
            env: None,
            flow: Some(OAuthFlowConfig::Pkce),
            is_subscription: Some(true),
            device_authorization_url: None,
            authorize_url: Some("https://auth.openai.com/authorize".to_string()),
            token_url: Some("https://auth.openai.com/oauth/token".to_string()),
            client_id: Some("codex-cli".to_string()),
            scope: Some("openid profile email".to_string()),
            redirect_uri: Some("http://localhost:1455/auth/callback".to_string()),
            loopback: Some(true),
            preset: None,
        }),
        "github-copilot" => Some(AuthProviderConfig {
            id: String::new(),
            method: AuthMethodConfig::OAuth,
            display_name: String::new(),
            login_label: Some("Sign in with GitHub".to_string()),
            env: None,
            flow: Some(OAuthFlowConfig::Device),
            is_subscription: Some(true),
            device_authorization_url: Some("https://github.com/login/device/code".to_string()),
            authorize_url: None,
            token_url: Some("https://github.com/login/oauth/access_token".to_string()),
            client_id: Some("Iv1.b507a08c87ec2d4b".to_string()),
            scope: Some("read:user".to_string()),
            redirect_uri: None,
            loopback: Some(false),
            preset: None,
        }),
        "google-vertex" => Some(AuthProviderConfig {
            id: String::new(),
            method: AuthMethodConfig::OAuth,
            display_name: String::new(),
            login_label: Some("Sign in with Google".to_string()),
            env: None,
            flow: Some(OAuthFlowConfig::Pkce),
            is_subscription: Some(false),
            device_authorization_url: None,
            authorize_url: Some("https://accounts.google.com/o/oauth2/v2/auth".to_string()),
            token_url: Some("https://oauth2.googleapis.com/token".to_string()),
            client_id: Some("vertex-cli".to_string()),
            scope: Some("https://www.googleapis.com/auth/cloud-platform".to_string()),
            redirect_uri: Some("http://localhost:8085/oauth/callback".to_string()),
            loopback: Some(false),
            preset: None,
        }),
        _ => None,
    }
}

/// Expand `preset` on a provider declaration: missing OAuth fields are
/// filled from the named template; explicit fields always win. The
/// `preset` marker is cleared on success so downstream validation sees a
/// plain declaration. Unknown preset names are an error.
pub fn expand_auth_preset(mut provider: AuthProviderConfig) -> Result<AuthProviderConfig, String> {
    let Some(preset_name) = provider.preset.clone() else {
        return Ok(provider);
    };
    let template =
        oauth_preset_template(&preset_name).ok_or_else(|| format!("unknown auth preset '{preset_name}'"))?;
    if provider.flow.is_none() {
        provider.flow = template.flow;
    }
    if provider.is_subscription.is_none() {
        provider.is_subscription = template.is_subscription;
    }
    if provider.device_authorization_url.is_none() {
        provider.device_authorization_url = template.device_authorization_url;
    }
    if provider.authorize_url.is_none() {
        provider.authorize_url = template.authorize_url;
    }
    if provider.token_url.is_none() {
        provider.token_url = template.token_url;
    }
    if provider.client_id.is_none() {
        provider.client_id = template.client_id;
    }
    if provider.scope.is_none() {
        provider.scope = template.scope;
    }
    if provider.redirect_uri.is_none() {
        provider.redirect_uri = template.redirect_uri;
    }
    if provider.loopback.is_none() {
        provider.loopback = template.loopback;
    }
    if provider.login_label.is_none() {
        provider.login_label = template.login_label;
    }
    // A preset implies OAuth unless the declaration says otherwise.
    if provider.method == AuthMethodConfig::ApiKey
        && (provider.flow.is_some()
            || provider.token_url.is_some()
            || provider.client_id.is_some())
    {
        provider.method = AuthMethodConfig::OAuth;
    }
    provider.preset = None;
    Ok(provider)
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
            preset: None,
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

    #[test]
    fn preset_names_cover_all_templates() {
        for name in oauth_preset_names() {
            assert!(oauth_preset_template(name).is_some(), "{name}");
        }
        assert!(oauth_preset_template("nope").is_none());
    }

    #[test]
    fn expand_preset_fills_missing_fields_and_clears_marker() {
        let provider = AuthProviderConfig {
            id: "codex".to_string(),
            method: AuthMethodConfig::ApiKey,
            display_name: "Codex".to_string(),
            login_label: None,
            env: None,
            flow: None,
            is_subscription: None,
            device_authorization_url: None,
            authorize_url: None,
            token_url: None,
            client_id: None,
            scope: None,
            redirect_uri: None,
            loopback: None,
            preset: Some("openai-codex".to_string()),
        };
        let expanded = expand_auth_preset(provider).expect("expand");
        assert_eq!(expanded.preset, None);
        assert_eq!(expanded.method, AuthMethodConfig::OAuth);
        assert_eq!(expanded.flow, Some(OAuthFlowConfig::Pkce));
        assert!(expanded.token_url.is_some());
    }

    #[test]
    fn expand_preset_keeps_explicit_fields() {
        let provider = AuthProviderConfig {
            id: "codex".to_string(),
            method: AuthMethodConfig::OAuth,
            display_name: "Codex".to_string(),
            login_label: None,
            env: None,
            flow: Some(OAuthFlowConfig::Pkce),
            is_subscription: None,
            device_authorization_url: None,
            authorize_url: None,
            token_url: Some("https://custom.test/token".to_string()),
            client_id: None,
            scope: None,
            redirect_uri: None,
            loopback: None,
            preset: Some("openai-codex".to_string()),
        };
        let expanded = expand_auth_preset(provider).expect("expand");
        assert_eq!(
            expanded.token_url.as_deref(),
            Some("https://custom.test/token")
        );
    }

    #[test]
    fn expand_unknown_preset_fails() {
        let provider = AuthProviderConfig {
            preset: Some("nope".to_string()),
            ..provider()
        };
        assert!(expand_auth_preset(provider).is_err());
    }
}

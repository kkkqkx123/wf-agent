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

/// Encoding of the OAuth token endpoint request body.
///
/// `form` is the RFC 6749 default and covers all currently built-in device
/// presets. `json` sends the same fields as a JSON object for providers
/// that reject form bodies (e.g. Anthropic). `None` in the declaration
/// means `form`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthTokenContentTypeConfig {
    #[default]
    Form,
    Json,
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
    /// Token endpoint body encoding (`form` default, `json` for providers
    /// that reject form bodies). Applies to the PKCE code exchange, the
    /// device token poll and refresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_content_type: Option<OAuthTokenContentTypeConfig>,
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
    vec![
        "openai-codex",
        "github-copilot",
        "google-vertex",
        "xai",
        "kimi-coding",
        "anthropic",
        "meta-muse",
    ]
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
            scope: Some("openid profile email offline_access".to_string()),
            redirect_uri: Some("http://localhost:1455/auth/callback".to_string()),
            loopback: Some(true),
            token_content_type: None,
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
            token_content_type: None,
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
            token_content_type: None,
            preset: None,
        }),
        // xAI device flow: standard RFC 8628 form bodies on both the
        // device authorization and token endpoints.
        "xai" => Some(AuthProviderConfig {
            id: String::new(),
            method: AuthMethodConfig::OAuth,
            display_name: String::new(),
            login_label: Some("Sign in with xAI".to_string()),
            env: None,
            flow: Some(OAuthFlowConfig::Device),
            is_subscription: Some(true),
            device_authorization_url: Some("https://auth.x.ai/oauth2/device/code".to_string()),
            authorize_url: None,
            token_url: Some("https://auth.x.ai/oauth2/token".to_string()),
            client_id: Some("b1a00492-073a-47ea-816f-4c329264a828".to_string()),
            scope: Some(
                "openid profile email offline_access grok-cli:access api:access".to_string(),
            ),
            redirect_uri: None,
            loopback: Some(false),
            token_content_type: None,
            preset: None,
        }),
        // Anthropic PKCE flow (Claude Pro/Max). The endpoints and client
        // follow the Claude Code client; the login behavior (browser /
        // copy-code choice, JSON token body carrying `state`) lives in the
        // dedicated in-tree driver, not in the generic PKCE flow.
        "anthropic" => Some(AuthProviderConfig {
            id: String::new(),
            method: AuthMethodConfig::OAuth,
            display_name: String::new(),
            login_label: Some("Sign in with Claude Pro/Max".to_string()),
            env: None,
            flow: Some(OAuthFlowConfig::Pkce),
            is_subscription: Some(true),
            device_authorization_url: None,
            authorize_url: Some("https://claude.ai/oauth/authorize".to_string()),
            token_url: Some("https://platform.claude.com/v1/oauth/token".to_string()),
            client_id: Some("0d1c250a-e61b-44d9-88ed-5944d1962f5e".to_string()),
            scope: Some("org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload".to_string()),
            redirect_uri: Some("https://platform.claude.com/oauth/code/callback".to_string()),
            loopback: None,
            token_content_type: Some(OAuthTokenContentTypeConfig::Json),
            preset: None,
        }),
        // Meta device flow (Muse subscription). The device exchange is
        // standard; the identity-to-API-key mint lives in the dedicated
        // in-tree driver.
        "meta-muse" => Some(AuthProviderConfig {
            id: String::new(),
            method: AuthMethodConfig::OAuth,
            display_name: String::new(),
            login_label: Some("Sign in with Meta".to_string()),
            env: None,
            flow: Some(OAuthFlowConfig::Device),
            is_subscription: Some(true),
            device_authorization_url: Some(
                "https://auth.meta.com/oidc/device/authorization/".to_string(),
            ),
            authorize_url: None,
            token_url: Some("https://auth.meta.com/oidc/device/token/".to_string()),
            client_id: Some("1031625952748946".to_string()),
            scope: None,
            redirect_uri: None,
            loopback: Some(false),
            token_content_type: None,
            preset: None,
        }),
        // Kimi device flow: RFC 8628 form bodies against the Kimi auth
        // host. No scope is sent; the server issues coding-scoped tokens.
        "kimi-coding" => Some(AuthProviderConfig {
            id: String::new(),
            method: AuthMethodConfig::OAuth,
            display_name: String::new(),
            login_label: Some("Sign in with Kimi".to_string()),
            env: None,
            flow: Some(OAuthFlowConfig::Device),
            is_subscription: Some(true),
            device_authorization_url: Some(
                "https://auth.kimi.com/api/oauth/device_authorization".to_string(),
            ),
            authorize_url: None,
            token_url: Some("https://auth.kimi.com/api/oauth/token".to_string()),
            client_id: Some("17e5f671-d194-4dfb-9706-5516cb48c098".to_string()),
            scope: None,
            redirect_uri: None,
            loopback: Some(false),
            token_content_type: None,
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
    let template = oauth_preset_template(&preset_name)
        .ok_or_else(|| format!("unknown auth preset '{preset_name}'"))?;
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
    if provider.token_content_type.is_none() {
        provider.token_content_type = template.token_content_type;
    }
    if provider.login_label.is_none() {
        provider.login_label = template.login_label;
    }
    // A preset implies OAuth unless the declaration says otherwise.
    if provider.method == AuthMethodConfig::ApiKey
        && (provider.flow.is_some() || provider.token_url.is_some() || provider.client_id.is_some())
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
            token_content_type: None,
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
            token_content_type: None,
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
            token_content_type: None,
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

    #[test]
    fn standard_device_presets_expand_to_valid_declarations() {
        for name in ["xai", "kimi-coding"] {
            let template = oauth_preset_template(name).expect(name);
            assert_eq!(template.flow, Some(OAuthFlowConfig::Device));
            assert!(template.device_authorization_url.is_some(), "{name}");
            assert!(template.token_url.is_some(), "{name}");
            assert!(template.client_id.is_some(), "{name}");
            let provider = AuthProviderConfig {
                id: name.to_string(),
                method: AuthMethodConfig::ApiKey,
                display_name: name.to_string(),
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
                token_content_type: None,
                preset: Some(name.to_string()),
            };
            let expanded = expand_auth_preset(provider).expect("expand");
            assert_eq!(expanded.preset, None);
            assert_eq!(expanded.method, AuthMethodConfig::OAuth);
        }
        let xai = oauth_preset_template("xai").expect("xai");
        assert!(
            xai.scope
                .as_deref()
                .unwrap_or_default()
                .contains("grok-cli:access"),
            "xai scope must cover grok-cli"
        );
        // Kimi sends no scope on the device authorization request.
        assert!(oauth_preset_template("kimi-coding")
            .expect("kimi")
            .scope
            .is_none());
    }

    #[test]
    fn json_token_content_type_roundtrips_and_expands() {
        let toml = r#"
id = "acme"
method = "oauth"
display_name = "Acme"
flow = "pkce"
authorize_url = "https://acme.test/authorize"
token_url = "https://acme.test/token"
client_id = "cid"
redirect_uri = "http://127.0.0.1:8080/callback"
token_content_type = "json"
"#;
        let parsed: AuthProviderConfig = toml::from_str(toml).expect("deserialize");
        assert_eq!(
            parsed.token_content_type,
            Some(OAuthTokenContentTypeConfig::Json)
        );
        let provider = AuthProviderConfig {
            token_content_type: Some(OAuthTokenContentTypeConfig::Json),
            preset: Some("openai-codex".to_string()),
            ..provider()
        };
        let expanded = expand_auth_preset(provider).expect("expand");
        // Explicit fields always win over the preset (whose default is form).
        assert_eq!(
            expanded.token_content_type,
            Some(OAuthTokenContentTypeConfig::Json)
        );
    }
}

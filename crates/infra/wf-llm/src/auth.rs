//! Built-in provider authentication surfaces driven by declaration.
//!
//! Hosts declare a provider's auth parameters (environment variable, OAuth
//! endpoints, client id, scopes, subscription flag) in configuration; this
//! module turns each declaration into the [`llm_auth::ProviderAuth`] the
//! registry expects. No provider is hard-coded here: the same declaration
//! shape covers API-key providers and subscription OAuth providers.

use std::sync::Arc;

use llm_auth::auth::{ApiKeyAuth, ModelAuth, OAuthAuth, ProviderAuth, ResolveInput};
use llm_auth::credential::OAuthCredential;
use llm_auth::error::{AuthError, AuthResult};
use llm_auth::interaction::AuthPrompt;
use llm_auth::oauth::device_flow::{self, DeviceFlowConfig};
use llm_auth::oauth::pkce::{self, PkceConfig};
use llm_auth::session::LoginFlow;

/// API-key authentication backed by one environment variable.
///
/// Resolution order: the stored credential wins, then the declared
/// environment variable. Interactive login pastes a key into the store.
#[derive(Debug, Clone)]
pub struct EnvApiKeyAuth {
    /// Environment variable consulted when nothing is stored.
    pub env_name: String,
    /// Display name, e.g. "Anthropic API key".
    pub display_name: String,
}

impl EnvApiKeyAuth {
    pub fn new(env_name: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            env_name: env_name.into(),
            display_name: display_name.into(),
        }
    }

    /// The provider surface for one declaration.
    pub fn provider_auth(
        env_name: impl Into<String>,
        display_name: impl Into<String>,
    ) -> ProviderAuth {
        ProviderAuth::api_key_only(Arc::new(Self::new(env_name, display_name)))
    }
}

#[async_trait::async_trait]
impl ApiKeyAuth for EnvApiKeyAuth {
    fn name(&self) -> String {
        self.display_name.clone()
    }

    async fn resolve(&self, input: ResolveInput<'_>) -> AuthResult<Option<ModelAuth>> {
        if let Some(cred) = input.credential {
            if let Some(key) = cred.as_api_key().ok().and_then(|c| c.key.clone()) {
                return Ok(Some(ModelAuth {
                    api_key: Some(key),
                    source: Some("stored credential".to_string()),
                    ..Default::default()
                }));
            }
        }
        match input.ctx.env(&self.env_name).await {
            Some(key) => Ok(Some(ModelAuth {
                api_key: Some(key),
                source: Some(self.env_name.clone()),
                ..Default::default()
            })),
            None => Ok(None),
        }
    }
}

/// Which OAuth flow a declared provider uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaredFlow {
    /// Device authorization grant (RFC 8628): short user code plus a
    /// verification URL, polled by the client.
    Device,
    /// Authorization code with PKCE (RFC 7636): browser redirect, either
    /// through a loopback listener or a pasted callback URL.
    Pkce,
}

/// Declared OAuth parameters for one subscription provider.
#[derive(Debug, Clone)]
pub struct DeclaredOAuthConfig {
    /// Display name, e.g. "Anthropic (Claude Pro/Max)".
    pub display_name: String,
    /// Which flow the provider supports.
    pub flow: DeclaredFlow,
    /// True when access through this flow is subscription-backed.
    pub is_subscription: bool,
    /// Device-flow endpoints (`flow = Device`).
    pub device_authorization_url: Option<String>,
    /// Token endpoint (both flows).
    pub token_url: String,
    /// Authorization endpoint (`flow = Pkce`).
    pub authorize_url: Option<String>,
    /// OAuth client id.
    pub client_id: String,
    /// Requested scopes.
    pub scope: Option<String>,
    /// Redirect URI (`flow = Pkce`); the loopback mode rewrites its port.
    pub redirect_uri: Option<String>,
    /// Bind a loopback listener instead of asking for a pasted callback.
    pub loopback: bool,
    /// Selector label for the login option.
    pub login_label: Option<String>,
}

/// OAuth authentication driven entirely by declaration.
#[derive(Debug, Clone)]
pub struct DeclaredOAuth {
    config: DeclaredOAuthConfig,
}

impl DeclaredOAuth {
    pub fn new(config: DeclaredOAuthConfig) -> Self {
        Self { config }
    }

    /// The provider surface for one declaration.
    pub fn provider_auth(config: DeclaredOAuthConfig) -> ProviderAuth {
        ProviderAuth::oauth_only(Arc::new(Self::new(config)))
    }

    fn device_config(&self) -> AuthResult<DeviceFlowConfig> {
        Ok(DeviceFlowConfig {
            device_authorization_url: self.config.device_authorization_url.clone().ok_or_else(
                || {
                    AuthError::MalformedCredential(
                        "device flow requires device_authorization_url".to_string(),
                    )
                },
            )?,
            token_url: self.config.token_url.clone(),
            client_id: self.config.client_id.clone(),
            scope: self.config.scope.clone(),
            is_subscription: self.config.is_subscription,
        })
    }

    fn pkce_config(&self) -> AuthResult<PkceConfig> {
        Ok(PkceConfig {
            authorize_url: self.config.authorize_url.clone().ok_or_else(|| {
                AuthError::MalformedCredential("pkce flow requires authorize_url".to_string())
            })?,
            token_url: self.config.token_url.clone(),
            client_id: self.config.client_id.clone(),
            redirect_uri: self.config.redirect_uri.clone().ok_or_else(|| {
                AuthError::MalformedCredential("pkce flow requires redirect_uri".to_string())
            })?,
            scope: self.config.scope.clone(),
            loopback: self.config.loopback,
            is_subscription: self.config.is_subscription,
        })
    }
}

#[async_trait::async_trait]
impl OAuthAuth for DeclaredOAuth {
    fn name(&self) -> String {
        self.config.display_name.clone()
    }

    fn is_subscription(&self) -> bool {
        self.config.is_subscription
    }

    fn login_label(&self) -> Option<String> {
        self.config.login_label.clone()
    }

    async fn begin_login(&self) -> AuthResult<LoginFlow> {
        match self.config.flow {
            DeclaredFlow::Device => Ok(LoginFlow::Device(
                device_flow::start(self.device_config()?, reqwest::Client::new(), now_ms()).await?,
            )),
            DeclaredFlow::Pkce => {
                let pending = pkce::start(self.pkce_config()?).await?;
                if self.config.loopback {
                    Ok(LoginFlow::PkceLoopback(pending))
                } else {
                    Ok(LoginFlow::PkcePaste(pending))
                }
            }
        }
    }

    async fn refresh(&self, credential: &OAuthCredential) -> AuthResult<OAuthCredential> {
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", credential.refresh.as_str()),
            ("client_id", self.config.client_id.as_str()),
        ];
        let response = reqwest::Client::new()
            .post(&self.config.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(llm_auth::oauth::form_body(&form))
            .send()
            .await
            .map_err(|e| AuthError::Transport(e.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| AuthError::Transport(e.to_string()))?;
        let mut token: llm_auth::TokenResponse = serde_json::from_str(&body).map_err(|e| {
            AuthError::Transport(format!("token response: {e} (HTTP {})", status.as_u16()))
        })?;
        if token.refresh_token.is_none() {
            token.refresh_token = Some(credential.refresh.clone());
        }
        let mut fresh =
            llm_auth::credential_from_token(&token, self.config.is_subscription, now_ms())?;
        fresh.extra = credential.extra.clone();
        Ok(fresh)
    }

    async fn to_auth(&self, credential: &OAuthCredential) -> AuthResult<ModelAuth> {
        Ok(ModelAuth {
            api_key: Some(credential.access.clone()),
            is_subscription: self.config.is_subscription,
            source: Some("OAuth".to_string()),
            ..Default::default()
        })
    }
}

/// The ambient prompt used when collecting a pasted API key.
pub fn api_key_prompt(display_name: &str) -> AuthPrompt {
    AuthPrompt::Secret {
        message: format!("Paste your {display_name}:"),
        placeholder: None,
    }
}

/// Built-in OAuth presets: well-known subscription providers as declarative
/// configs. Hosts reference them via `preset = "openai-codex"` in
/// `AuthProviderConfig`; explicit fields always win over the preset. No
/// provider is hard-wired into the auth flow itself: presets are pure data
/// constructors over [`DeclaredOAuthConfig`].
pub mod oauth_presets {
    use super::{DeclaredFlow, DeclaredOAuthConfig};

    /// OpenAI Codex (ChatGPT subscription) OAuth preset (PKCE, loopback).
    pub fn openai_codex() -> DeclaredOAuthConfig {
        DeclaredOAuthConfig {
            display_name: "OpenAI Codex".to_string(),
            flow: DeclaredFlow::Pkce,
            is_subscription: true,
            device_authorization_url: None,
            token_url: "https://auth.openai.com/oauth/token".to_string(),
            authorize_url: Some("https://auth.openai.com/authorize".to_string()),
            client_id: "codex-cli".to_string(),
            scope: Some("openid profile email".to_string()),
            redirect_uri: Some("http://localhost:1455/auth/callback".to_string()),
            loopback: true,
            login_label: Some("Sign in with ChatGPT".to_string()),
        }
    }

    /// GitHub Copilot OAuth preset (device flow, subscription-backed).
    pub fn github_copilot() -> DeclaredOAuthConfig {
        DeclaredOAuthConfig {
            display_name: "GitHub Copilot".to_string(),
            flow: DeclaredFlow::Device,
            is_subscription: true,
            device_authorization_url: Some(
                "https://github.com/login/device/code".to_string(),
            ),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            authorize_url: None,
            client_id: "Iv1.b507a08c87ec2d4b".to_string(),
            scope: Some("read:user".to_string()),
            redirect_uri: None,
            loopback: false,
            login_label: Some("Sign in with GitHub".to_string()),
        }
    }

    /// Google Vertex (Gemini) OAuth preset (PKCE, pasted callback).
    /// Uses the Google OAuth endpoints with the Cloud Platform scope; the
    /// client id is a placeholder hosts override with their own credential.
    pub fn google_vertex() -> DeclaredOAuthConfig {
        DeclaredOAuthConfig {
            display_name: "Google Vertex".to_string(),
            flow: DeclaredFlow::Pkce,
            is_subscription: false,
            device_authorization_url: None,
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            authorize_url: Some("https://accounts.google.com/o/oauth2/v2/auth".to_string()),
            client_id: "vertex-cli".to_string(),
            scope: Some("https://www.googleapis.com/auth/cloud-platform".to_string()),
            redirect_uri: Some("http://localhost:8085/oauth/callback".to_string()),
            loopback: false,
            login_label: Some("Sign in with Google".to_string()),
        }
    }

    /// Resolve a preset name to its declarative config.
    /// Returns `None` for unknown names so callers can fail loudly.
    pub fn preset_named(name: &str) -> Option<DeclaredOAuthConfig> {
        match name {
            "openai-codex" => Some(openai_codex()),
            "github-copilot" => Some(github_copilot()),
            "google-vertex" => Some(google_vertex()),
            _ => None,
        }
    }

    /// All built-in preset names in stable order.
    pub fn preset_names() -> Vec<&'static str> {
        vec!["openai-codex", "github-copilot", "google-vertex"]
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm_auth::auth::AuthContext;
    use llm_auth::credential::{ApiKeyCredential, Credential};

    fn device_config() -> DeclaredOAuthConfig {
        DeclaredOAuthConfig {
            display_name: "Example Sub".to_string(),
            flow: DeclaredFlow::Device,
            is_subscription: true,
            device_authorization_url: Some("https://example.test/device".to_string()),
            token_url: "https://example.test/token".to_string(),
            authorize_url: None,
            client_id: "cid".to_string(),
            scope: Some("api".to_string()),
            redirect_uri: None,
            loopback: false,
            login_label: Some("Sign in with Example Sub".to_string()),
        }
    }

    #[test]
    fn env_auth_reports_display_name_and_prompt() {
        let auth = EnvApiKeyAuth::new("EXAMPLE_KEY", "Example API key");
        assert_eq!(auth.name(), "Example API key");
        assert!(matches!(
            api_key_prompt(&auth.name()),
            AuthPrompt::Secret { .. }
        ));
    }

    #[tokio::test]
    async fn env_auth_prefers_stored_credential_over_environment() {
        let auth = EnvApiKeyAuth::new("EXAMPLE_KEY", "Example API key");
        let stored = Credential::ApiKey(ApiKeyCredential {
            key: Some("sk-stored".to_string()),
            env: Default::default(),
        });
        let ctx: Arc<dyn AuthContext> = Arc::new(llm_auth::StdAuthContext);
        let resolved = auth
            .resolve(ResolveInput {
                ctx: &ctx,
                credential: Some(&stored),
            })
            .await
            .expect("resolve")
            .expect("configured");
        assert_eq!(resolved.api_key.as_deref(), Some("sk-stored"));
    }

    #[tokio::test]
    async fn declared_oauth_requires_declared_endpoints() {
        let mut config = device_config();
        config.device_authorization_url = None;
        let auth = DeclaredOAuth::new(config);
        let err = auth.begin_login().await.expect_err("must fail");
        assert!(matches!(err, AuthError::MalformedCredential(_)));
    }

    #[tokio::test]
    async fn declared_oauth_carries_subscription_flag_into_auth() {
        let auth = DeclaredOAuth::new(device_config());
        let credential = OAuthCredential {
            refresh: "r".to_string(),
            access: "a".to_string(),
            expires: 0,
            is_subscription: true,
            extra: Default::default(),
        };
        let resolved = auth.to_auth(&credential).await.expect("auth");
        assert!(resolved.is_subscription);
        assert_eq!(resolved.api_key.as_deref(), Some("a"));
    }

    #[test]
    fn provider_surfaces_expose_the_declared_method() {
        let key_surface = EnvApiKeyAuth::provider_auth("EXAMPLE_KEY", "Example API key");
        assert!(key_surface.api_key.is_some());
        assert!(key_surface.oauth.is_none());

        let oauth_surface = DeclaredOAuth::provider_auth(device_config());
        assert!(oauth_surface.oauth.is_some());
        assert!(oauth_surface.api_key.is_none());
    }

    #[test]
    fn oauth_presets_cover_all_names() {
        for name in oauth_presets::preset_names() {
            let config = oauth_presets::preset_named(name).expect(name);
            assert!(!config.token_url.is_empty(), "{name}");
            assert!(!config.client_id.is_empty(), "{name}");
        }
        assert!(oauth_presets::preset_named("nope").is_none());
    }

    #[test]
    fn oauth_presets_carry_expected_flows() {
        let codex = oauth_presets::openai_codex();
        assert_eq!(codex.flow, DeclaredFlow::Pkce);
        assert!(codex.is_subscription);

        let copilot = oauth_presets::github_copilot();
        assert_eq!(copilot.flow, DeclaredFlow::Device);
        assert!(copilot.is_subscription);
        assert!(copilot.device_authorization_url.is_some());

        let vertex = oauth_presets::google_vertex();
        assert_eq!(vertex.flow, DeclaredFlow::Pkce);
    }
}

//! First-party provider-specific OAuth implementations.
//!
//! Generic RFC flows stay behind `DeclaredOAuth` in this crate's `auth`
//! module; providers whose token handling deviates from the generic shape
//! get a dedicated in-tree driver here, selected by preset name at
//! bootstrap. Everything runs in-process and versions with the binary: no
//! cdylib, no scan directory, no per-platform artifact. The drivers speak
//! the host `ExternalFlowDriver` contract, so sessions, polling, TUI panels
//! and the credential store keep working unchanged.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use llm_auth::auth::{ModelAuth, OAuthAuth, ProviderAuth};
use llm_auth::credential::{Credential, OAuthCredential};
use llm_auth::error::{AuthError, AuthResult};
use llm_auth::interaction::{AuthEvent, AuthPrompt, AuthPromptOption};
use llm_auth::session::{
    ExternalFlowDriver, ExternalPending, ExternalState, ExternalStep, LoginFlow,
};

/// Per-call HTTP timeout for provider OAuth endpoints.
const HTTP_TIMEOUT_SECS: u64 = 30;
/// Fallback device polling interval when the server omits `interval`.
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
/// Fallback device code lifetime when the server omits `expires_in`.
const DEFAULT_DEVICE_EXPIRY_SECS: u64 = 900;

fn http_client() -> AuthResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|e| AuthError::Transport(format!("oauth http client: {e}")))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Read a response body as JSON, mapping transport and shape failures.
async fn read_json(response: reqwest::Response) -> AuthResult<(u16, Value)> {
    let status = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|e| AuthError::Transport(format!("oauth response unreadable: {e}")))?;
    if text.trim().is_empty() {
        return Ok((status, Value::Null));
    }
    let body: Value = serde_json::from_str(&text)
        .map_err(|e| AuthError::Transport(format!("oauth response is not JSON: {e}")))?;
    Ok((status, body))
}

fn required_str(body: &Value, field: &str) -> Result<String, String> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("oauth response carries no {field}"))
}

fn optional_u64(body: &Value, field: &str) -> Option<u64> {
    body.get(field).and_then(Value::as_u64).filter(|v| *v > 0)
}

/// Form-encode ordered pairs (the shared reqwest build has no `form`
/// feature, so encoding stays explicit at the call site).
fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut encoder = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        encoder.append_pair(key, value);
    }
    encoder.finish()
}

/// HTTPS-only check for server-advertised verification URIs, so a hostile
/// response cannot make the host open an arbitrary scheme.
fn trusted_https_url(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() == "https" {
        Some(url.to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// GitHub Copilot: device flow plus a Copilot-token second exchange.
// ---------------------------------------------------------------------------

/// Client id of the GitHub OAuth app used for the device flow.
pub const GITHUB_COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ec2d4b";
/// Scope requested on the device authorization call.
pub const GITHUB_COPILOT_SCOPE: &str = "read:user";
/// Headers identifying the client on GitHub and Copilot endpoints.
const COPILOT_UA: &str = "GitHubCopilotChat/0.35.0";
/// Copilot tokens live about as long as the server advertises; the stored
/// expiry is pulled back so a request never starts on a dying token.
const COPILOT_EXPIRY_SKEW_MS: i64 = 5 * 60 * 1000;

/// Endpoints derived from one GitHub domain (`github.com` by default).
/// All fields are public so tests (and proxy front doors) can point the
/// driver at a mock server; production always derives them from the domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotUrls {
    pub device_code_url: String,
    pub access_token_url: String,
    pub copilot_token_url: String,
}

fn copilot_urls(domain: &str) -> CopilotUrls {
    CopilotUrls {
        device_code_url: format!("https://{domain}/login/device/code"),
        access_token_url: format!("https://{domain}/login/oauth/access_token"),
        copilot_token_url: format!("https://api.{domain}/copilot_internal/v2/token"),
    }
}

/// Normalize the enterprise domain prompt: blank means `github.com`.
/// Returns `None` for the default host.
fn normalize_domain(input: &str) -> Result<Option<String>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let host = without_scheme.split('/').next().unwrap_or_default().trim();
    if host.is_empty()
        || host.len() > 253
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return Err(format!("invalid GitHub Enterprise domain: {trimmed}"));
    }
    if host == "github.com" {
        return Ok(None);
    }
    Ok(Some(host.to_owned()))
}

fn copilot_domain_label(domain: Option<&str>) -> &str {
    domain.unwrap_or("github.com")
}

/// Provider parameters: explicit declaration values win, pi defaults fill
/// the gaps so a bare `preset = "github-copilot"` works.
#[derive(Debug, Clone)]
pub struct GithubCopilotParams {
    pub client_id: String,
    pub scope: String,
    /// Endpoint override for tests and proxy front doors; `None` derives
    /// all three URLs from the login domain.
    pub endpoints: Option<CopilotUrls>,
}

impl GithubCopilotParams {
    pub fn from_declared(client_id: Option<String>, scope: Option<String>) -> Self {
        Self {
            client_id: client_id
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| GITHUB_COPILOT_CLIENT_ID.to_owned()),
            scope: scope
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| GITHUB_COPILOT_SCOPE.to_owned()),
            endpoints: None,
        }
    }
}

/// Device authorization response of the GitHub device flow.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CopilotDeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval_secs: Option<u64>,
    expires_in_secs: Option<u64>,
}

fn parse_device_code(body: &Value) -> Result<CopilotDeviceCode, String> {
    let verification_uri = required_str(body, "verification_uri").and_then(|uri| {
        trusted_https_url(&uri).ok_or_else(|| "untrusted verification_uri".to_owned())
    })?;
    Ok(CopilotDeviceCode {
        device_code: required_str(body, "device_code")?,
        user_code: required_str(body, "user_code")?,
        verification_uri,
        interval_secs: optional_u64(body, "interval"),
        expires_in_secs: optional_u64(body, "expires_in"),
    })
}

/// Outcome of one device token poll attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DevicePoll {
    Pending,
    SlowDown(Option<u64>),
    Denied(String),
    Expired,
    Complete(String),
    Failed(String),
}

fn parse_device_poll(status: u16, body: &Value) -> DevicePoll {
    if let Ok(token) = required_str(body, "access_token") {
        return DevicePoll::Complete(token);
    }
    if status != 200 {
        let detail = body
            .get("error_description")
            .and_then(Value::as_str)
            .or_else(|| body.get("error").and_then(Value::as_str))
            .or_else(|| body.get("message").and_then(Value::as_str))
            .unwrap_or("unknown error");
        return DevicePoll::Failed(format!(
            "device token request failed (HTTP {status}): {detail}"
        ));
    }
    match body.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => DevicePoll::Pending,
        Some("slow_down") => DevicePoll::SlowDown(optional_u64(body, "interval")),
        Some("access_denied") => DevicePoll::Denied(
            body.get("error_description")
                .and_then(Value::as_str)
                .unwrap_or("access denied")
                .to_owned(),
        ),
        Some("expired_token") => DevicePoll::Expired,
        Some(other) => {
            let detail = body
                .get("error_description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            DevicePoll::Failed(format!("device flow error '{other}': {detail}"))
        }
        None => DevicePoll::Failed("device token response carries no access_token".to_owned()),
    }
}

/// Derive the per-request Copilot base URL from the token's `proxy-ep`
/// claim (`proxy.X` becomes `https://api.X`); `None` keeps the provider
/// definition's base URL.
pub fn copilot_base_url(access_token: &str) -> Option<String> {
    let proxy_host = access_token.split(';').find_map(|part| {
        part.trim()
            .strip_prefix("proxy-ep=")
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
    })?;
    let api_host = proxy_host
        .strip_prefix("proxy.")
        .map(|rest| format!("api.{rest}"))
        .unwrap_or_else(|| proxy_host.to_owned());
    if api_host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        Some(format!("https://{api_host}"))
    } else {
        None
    }
}

/// GitHub Copilot subscription login: RFC 8628 device flow yielding a
/// GitHub OAuth token, then a second exchange for the Copilot API token.
/// The GitHub token is stored as `refresh` so the scheduler re-mints the
/// Copilot token through the normal refresh path.
#[derive(Debug, Clone)]
pub struct GithubCopilotAuth {
    display_name: String,
    login_label: Option<String>,
    params: GithubCopilotParams,
}

impl GithubCopilotAuth {
    pub fn new(
        display_name: impl Into<String>,
        login_label: Option<String>,
        params: GithubCopilotParams,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            login_label,
            params,
        }
    }

    /// The provider surface for one declaration.
    pub fn provider_auth(
        display_name: impl Into<String>,
        login_label: Option<String>,
        params: GithubCopilotParams,
    ) -> ProviderAuth {
        ProviderAuth::oauth_only(Arc::new(Self::new(display_name, login_label, params)))
    }

    fn endpoints(&self, domain: Option<&str>) -> CopilotUrls {
        self.params
            .endpoints
            .clone()
            .unwrap_or_else(|| copilot_urls(copilot_domain_label(domain)))
    }

    async fn start_device(
        &self,
        domain: Option<&str>,
    ) -> AuthResult<(CopilotUrls, CopilotDeviceCode, i64)> {
        let urls = self.endpoints(domain);
        let client = http_client()?;
        let form = form_body(&[
            ("client_id", self.params.client_id.as_str()),
            ("scope", self.params.scope.as_str()),
        ]);
        let (status, body) = read_json(
            client
                .post(&urls.device_code_url)
                .header("Accept", "application/json")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("User-Agent", COPILOT_UA)
                .body(form)
                .send()
                .await
                .map_err(|e| AuthError::Transport(format!("device authorization failed: {e}")))?,
        )
        .await?;
        if status != 200 {
            return Err(AuthError::Transport(format!(
                "device authorization failed (HTTP {status}): {body}"
            )));
        }
        let device = parse_device_code(&body)
            .map_err(|e| AuthError::Transport(format!("invalid device code response: {e}")))?;
        let expires_at_ms =
            now_ms() + (device.expires_in_secs.unwrap_or(DEFAULT_DEVICE_EXPIRY_SECS) as i64) * 1000;
        Ok((urls, device, expires_at_ms))
    }

    async fn poll_device(&self, urls: &CopilotUrls, device_code: &str) -> AuthResult<DevicePoll> {
        let client = http_client()?;
        let form = form_body(&[
            ("client_id", self.params.client_id.as_str()),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ]);
        let (status, body) = read_json(
            client
                .post(&urls.access_token_url)
                .header("Accept", "application/json")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("User-Agent", COPILOT_UA)
                .body(form)
                .send()
                .await
                .map_err(|e| AuthError::Transport(format!("device poll failed: {e}")))?,
        )
        .await?;
        Ok(parse_device_poll(status, &body))
    }

    /// Exchange a GitHub OAuth token for a Copilot API token.
    async fn exchange_copilot_token(
        urls: &CopilotUrls,
        github_token: &str,
    ) -> AuthResult<(String, i64)> {
        let client = http_client()?;
        let (status, body) = read_json(
            client
                .get(&urls.copilot_token_url)
                .header("Accept", "application/json")
                .header("Authorization", format!("Bearer {github_token}"))
                .header("User-Agent", COPILOT_UA)
                .header("Editor-Version", "vscode/1.107.0")
                .header("Editor-Plugin-Version", "copilot-chat/0.35.0")
                .header("Copilot-Integration-Id", "vscode-chat")
                .send()
                .await
                .map_err(|e| AuthError::Transport(format!("copilot token exchange failed: {e}")))?,
        )
        .await?;
        if status == 401 || status == 403 {
            return Err(AuthError::Login(
                "GitHub token rejected by the Copilot endpoint; sign in again".to_owned(),
            ));
        }
        if status != 200 {
            return Err(AuthError::Transport(format!(
                "copilot token exchange failed (HTTP {status}): {body}"
            )));
        }
        let token = required_str(&body, "token")
            .map_err(|e| AuthError::Transport(format!("invalid copilot token response: {e}")))?;
        let expires_at = body
            .get("expires_at")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                AuthError::Transport("copilot token response carries no expires_at".to_owned())
            })?;
        Ok((token, expires_at * 1000 - COPILOT_EXPIRY_SKEW_MS))
    }

    fn failed_step(message: String) -> ExternalStep {
        ExternalStep::Failed {
            error: message,
            events: Vec::new(),
        }
    }
}

#[async_trait::async_trait]
impl OAuthAuth for GithubCopilotAuth {
    fn name(&self) -> String {
        self.display_name.clone()
    }

    fn is_subscription(&self) -> bool {
        true
    }

    fn login_label(&self) -> Option<String> {
        self.login_label.clone()
    }

    async fn begin_login(&self) -> AuthResult<LoginFlow> {
        Ok(LoginFlow::External(ExternalPending::new(
            json!({ "stage": "awaiting_domain" }),
            Arc::new(self.clone()),
        )))
    }

    async fn refresh(&self, credential: &OAuthCredential) -> AuthResult<OAuthCredential> {
        let domain = credential
            .extra
            .get("enterprise_domain")
            .and_then(Value::as_str);
        let urls = self.endpoints(domain);
        let (access, expires) = Self::exchange_copilot_token(&urls, &credential.refresh)
            .await
            .map_err(|e| match e {
                AuthError::Login(message) => AuthError::Refresh(message),
                other => AuthError::Refresh(other.to_string()),
            })?;
        Ok(OAuthCredential {
            refresh: credential.refresh.clone(),
            access,
            expires,
            is_subscription: true,
            extra: credential.extra.clone(),
        })
    }

    async fn to_auth(&self, credential: &OAuthCredential) -> AuthResult<ModelAuth> {
        Ok(ModelAuth {
            api_key: Some(credential.access.clone()),
            base_url: copilot_base_url(&credential.access),
            is_subscription: true,
            source: Some("GitHub Copilot (OAuth)".to_owned()),
            ..Default::default()
        })
    }
}

#[async_trait::async_trait]
impl ExternalFlowDriver for GithubCopilotAuth {
    async fn initial(&self, _state: &ExternalState) -> AuthResult<ExternalStep> {
        Ok(ExternalStep::AwaitingInput {
            state: json!({ "stage": "awaiting_domain" }),
            prompt: AuthPrompt::Text {
                message: "GitHub Enterprise URL/domain (blank for github.com)".to_owned(),
                placeholder: Some("company.ghe.com".to_owned()),
            },
            events: Vec::new(),
        })
    }

    async fn submit(&self, state: &ExternalState, input: String) -> AuthResult<ExternalStep> {
        let stage = state
            .get("stage")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if stage != "awaiting_domain" {
            return Ok(Self::failed_step(
                "unexpected login input for the current step".to_owned(),
            ));
        }
        let domain = match normalize_domain(&input) {
            Ok(domain) => domain,
            Err(reason) => return Ok(Self::failed_step(reason)),
        };
        let (_urls, device, expires_at_ms) = match self.start_device(domain.as_deref()).await {
            Ok(started) => started,
            Err(e) => return Ok(Self::failed_step(e.to_string())),
        };
        let interval_secs = device.interval_secs.unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
        let remaining_secs = ((expires_at_ms - now_ms()).max(0) / 1000) as u64;
        Ok(ExternalStep::Pending {
            state: json!({
                "stage": "polling",
                "domain": domain,
                "device_code": device.device_code,
                "interval_secs": interval_secs,
                "expires_at_ms": expires_at_ms,
                "next_poll_at_ms": now_ms(),
            }),
            events: vec![AuthEvent::DeviceCode {
                user_code: device.user_code,
                verification_uri: device.verification_uri,
                interval_seconds: Some(interval_secs),
                expires_in_seconds: Some(remaining_secs),
            }],
        })
    }

    async fn poll(&self, state: &ExternalState) -> AuthResult<ExternalStep> {
        if state.get("stage").and_then(Value::as_str) != Some("polling") {
            return Ok(Self::failed_step(
                "unexpected poll for the current step".to_owned(),
            ));
        }
        let get_str = |key: &str| state.get(key).and_then(Value::as_str).map(str::to_owned);
        let get_u64 = |key: &str| state.get(key).and_then(Value::as_u64);
        let get_i64 = |key: &str| state.get(key).and_then(Value::as_i64);
        let Some(device_code) = get_str("device_code") else {
            return Ok(Self::failed_step(
                "login session carries no device code".to_owned(),
            ));
        };
        let interval_secs = get_u64("interval_secs").unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
        let expires_at_ms = get_i64("expires_at_ms").unwrap_or(0);
        let next_poll_at_ms = get_i64("next_poll_at_ms").unwrap_or(0);
        let now = now_ms();
        if now >= expires_at_ms {
            return Ok(Self::failed_step(
                "device code expired; sign in again".to_owned(),
            ));
        }
        if now < next_poll_at_ms {
            return Ok(ExternalStep::Pending {
                state: state.clone(),
                events: Vec::new(),
            });
        }
        let domain = get_str("domain");
        let urls = self.endpoints(domain.as_deref());
        let mut session = state.clone();
        let wait_secs = match self.poll_device(&urls, &device_code).await? {
            DevicePoll::Pending => interval_secs,
            DevicePoll::SlowDown(extra) => {
                let widened = extra.unwrap_or_else(|| interval_secs.saturating_add(5));
                if let Some(object) = session.as_object_mut() {
                    object.insert("interval_secs".to_owned(), json!(widened));
                }
                widened
            }
            DevicePoll::Denied(reason) => {
                return Ok(Self::failed_step(format!("login denied: {reason}")));
            }
            DevicePoll::Expired => {
                return Ok(Self::failed_step(
                    "device code expired; sign in again".to_owned(),
                ));
            }
            DevicePoll::Failed(reason) => return Ok(Self::failed_step(reason)),
            DevicePoll::Complete(github_token) => {
                let (access, expires) =
                    match Self::exchange_copilot_token(&urls, &github_token).await {
                        Ok(exchanged) => exchanged,
                        Err(e) => return Ok(Self::failed_step(e.to_string())),
                    };
                let mut extra = std::collections::BTreeMap::new();
                if let Some(domain) = domain {
                    extra.insert("enterprise_domain".to_owned(), Value::String(domain));
                }
                return Ok(ExternalStep::Completed {
                    credential: Credential::OAuth(OAuthCredential {
                        refresh: github_token,
                        access,
                        expires,
                        is_subscription: true,
                        extra,
                    }),
                });
            }
        };
        if let Some(object) = session.as_object_mut() {
            object.insert(
                "next_poll_at_ms".to_owned(),
                json!(now_ms() + (wait_secs as i64) * 1000),
            );
        }
        Ok(ExternalStep::Pending {
            state: session,
            events: Vec::new(),
        })
    }

    fn poll_interval(&self, state: &ExternalState) -> Duration {
        let secs = state
            .get("interval_secs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
        Duration::from_secs(secs.max(1))
    }
}

// ---------------------------------------------------------------------------
// Meta Muse: device flow plus an API-key mint second step.
// ---------------------------------------------------------------------------

/// Client id of the Muse Code CLI client.
pub const META_CLIENT_ID: &str = "1031625952748946";
/// Device authorization endpoint on the Meta auth host.
pub const META_DEVICE_AUTHORIZATION_URL: &str = "https://auth.meta.com/oidc/device/authorization/";
/// Device token endpoint on the Meta auth host.
pub const META_DEVICE_TOKEN_URL: &str = "https://auth.meta.com/oidc/device/token/";
/// Endpoint exchanging an identity token for a Model API key.
pub const META_API_KEY_MINT_URL: &str = "https://api.meta.ai/muse-code/key";
/// Minted keys live about a day; refresh re-mints through this path.
const META_API_KEY_LIFETIME_MS: i64 = 24 * 60 * 60 * 1000;

/// Provider parameters: explicit declaration values win, pi defaults fill
/// the gaps so a bare `preset = "meta-muse"` works.
#[derive(Debug, Clone)]
pub struct MetaMuseParams {
    pub device_authorization_url: String,
    pub token_url: String,
    pub client_id: String,
    /// Mint endpoint override for tests; `None` uses the Meta endpoint.
    pub mint_url: Option<String>,
}

impl MetaMuseParams {
    pub fn from_declared(
        device_authorization_url: Option<String>,
        token_url: Option<String>,
        client_id: Option<String>,
    ) -> Self {
        Self {
            device_authorization_url: device_authorization_url
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| META_DEVICE_AUTHORIZATION_URL.to_owned()),
            token_url: token_url
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| META_DEVICE_TOKEN_URL.to_owned()),
            client_id: client_id
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| META_CLIENT_ID.to_owned()),
            mint_url: None,
        }
    }

    fn mint_url(&self) -> &str {
        self.mint_url.as_deref().unwrap_or(META_API_KEY_MINT_URL)
    }
}

/// Meta Model API login: RFC 8628 device flow yielding an identity token,
/// then a mint call for the Model API key. The identity token is stored as
/// `refresh` and the minted key as `access`, so the scheduler re-mints the
/// key when it expires with no bespoke renewal machinery. The identity
/// token itself is not renewable: a 401/403 from mint means the session is
/// dead and the user must sign in again.
#[derive(Debug, Clone)]
pub struct MetaMuseAuth {
    display_name: String,
    login_label: Option<String>,
    params: MetaMuseParams,
}

impl MetaMuseAuth {
    pub fn new(
        display_name: impl Into<String>,
        login_label: Option<String>,
        params: MetaMuseParams,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            login_label,
            params,
        }
    }

    /// The provider surface for one declaration.
    pub fn provider_auth(
        display_name: impl Into<String>,
        login_label: Option<String>,
        params: MetaMuseParams,
    ) -> ProviderAuth {
        ProviderAuth::oauth_only(Arc::new(Self::new(display_name, login_label, params)))
    }

    async fn start_device(&self) -> AuthResult<(CopilotDeviceCode, i64)> {
        let client = http_client()?;
        let form = form_body(&[("client_id", self.params.client_id.as_str())]);
        let (status, body) = read_json(
            client
                .post(&self.params.device_authorization_url)
                .header("Accept", "application/json")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(form)
                .send()
                .await
                .map_err(|e| AuthError::Transport(format!("device authorization failed: {e}")))?,
        )
        .await?;
        if status != 200 {
            return Err(AuthError::Transport(format!(
                "device authorization failed (HTTP {status}): {body}"
            )));
        }
        // Prefer the pre-filled verification URI when the server supplies one.
        let mut with_complete = body.clone();
        if let Some(complete) = body.get("verification_uri_complete").cloned() {
            if let Some(object) = with_complete.as_object_mut() {
                object.insert("verification_uri".to_owned(), complete);
            }
        }
        let device = parse_device_code(&with_complete)
            .map_err(|e| AuthError::Transport(format!("invalid device code response: {e}")))?;
        let expires_at_ms =
            now_ms() + (device.expires_in_secs.unwrap_or(DEFAULT_DEVICE_EXPIRY_SECS) as i64) * 1000;
        Ok((device, expires_at_ms))
    }

    async fn poll_identity(&self, device_code: &str) -> AuthResult<DevicePoll> {
        let client = http_client()?;
        let form = form_body(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
            ("client_id", self.params.client_id.as_str()),
        ]);
        let (status, body) = read_json(
            client
                .post(&self.params.token_url)
                .header("Accept", "application/json")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(form)
                .send()
                .await
                .map_err(|e| AuthError::Transport(format!("device poll failed: {e}")))?,
        )
        .await?;
        Ok(parse_device_poll(status, &body))
    }

    /// Exchange an identity token for a Model API key.
    async fn mint_api_key(&self, identity_token: &str) -> AuthResult<(String, i64)> {
        let client = http_client()?;
        let (status, body) = read_json(
            client
                .post(self.params.mint_url())
                .header("Accept", "application/json")
                .header("Authorization", format!("Bearer {identity_token}"))
                .header("Content-Type", "application/json")
                .header("x-api-version", "1.0.0")
                .body("{}")
                .send()
                .await
                .map_err(|e| AuthError::Transport(format!("api key mint failed: {e}")))?,
        )
        .await?;
        if status == 401 || status == 403 {
            return Err(AuthError::Login(
                "Meta session expired; sign in again".to_owned(),
            ));
        }
        if status != 200 {
            return Err(AuthError::Transport(format!(
                "api key mint failed (HTTP {status}): {body}"
            )));
        }
        let api_key = required_str(&body, "api_key").map_err(|_| {
            let hint = body
                .get("action_url")
                .and_then(Value::as_str)
                .map(|url| format!(" Complete setup at {url}"))
                .unwrap_or_default();
            AuthError::Transport(format!("Meta did not issue an API key.{hint}"))
        })?;
        Ok((api_key, now_ms() + META_API_KEY_LIFETIME_MS))
    }

    fn failed_step(message: String) -> ExternalStep {
        ExternalStep::Failed {
            error: message,
            events: Vec::new(),
        }
    }
}

#[async_trait::async_trait]
impl OAuthAuth for MetaMuseAuth {
    fn name(&self) -> String {
        self.display_name.clone()
    }

    fn is_subscription(&self) -> bool {
        true
    }

    fn login_label(&self) -> Option<String> {
        self.login_label.clone()
    }

    async fn begin_login(&self) -> AuthResult<LoginFlow> {
        Ok(LoginFlow::External(ExternalPending::new(
            json!({ "stage": "awaiting_device" }),
            Arc::new(self.clone()),
        )))
    }

    async fn refresh(&self, credential: &OAuthCredential) -> AuthResult<OAuthCredential> {
        let (access, expires) = self
            .mint_api_key(&credential.refresh)
            .await
            .map_err(|e| AuthError::Refresh(e.to_string()))?;
        Ok(OAuthCredential {
            refresh: credential.refresh.clone(),
            access,
            expires,
            is_subscription: true,
            extra: credential.extra.clone(),
        })
    }

    async fn to_auth(&self, credential: &OAuthCredential) -> AuthResult<ModelAuth> {
        Ok(ModelAuth {
            api_key: Some(credential.access.clone()),
            is_subscription: true,
            source: Some("Meta (OAuth)".to_owned()),
            ..Default::default()
        })
    }
}

#[async_trait::async_trait]
impl ExternalFlowDriver for MetaMuseAuth {
    async fn initial(&self, _state: &ExternalState) -> AuthResult<ExternalStep> {
        let (device, expires_at_ms) = match self.start_device().await {
            Ok(started) => started,
            Err(e) => {
                return Ok(Self::failed_step(e.to_string()));
            }
        };
        let interval_secs = device.interval_secs.unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
        let remaining_secs = ((expires_at_ms - now_ms()).max(0) / 1000) as u64;
        Ok(ExternalStep::Pending {
            state: json!({
                "stage": "polling",
                "device_code": device.device_code,
                "interval_secs": interval_secs,
                "expires_at_ms": expires_at_ms,
                "next_poll_at_ms": now_ms(),
            }),
            events: vec![AuthEvent::DeviceCode {
                user_code: device.user_code,
                verification_uri: device.verification_uri,
                interval_seconds: Some(interval_secs),
                expires_in_seconds: Some(remaining_secs),
            }],
        })
    }

    async fn poll(&self, state: &ExternalState) -> AuthResult<ExternalStep> {
        if state.get("stage").and_then(Value::as_str) != Some("polling") {
            return Ok(Self::failed_step(
                "unexpected poll for the current step".to_owned(),
            ));
        }
        let device_code = state
            .get("device_code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if device_code.is_empty() {
            return Ok(Self::failed_step(
                "login session carries no device code".to_owned(),
            ));
        }
        let interval_secs = state
            .get("interval_secs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
        let expires_at_ms = state
            .get("expires_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let next_poll_at_ms = state
            .get("next_poll_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let now = now_ms();
        if now >= expires_at_ms {
            return Ok(Self::failed_step(
                "device authorization expired; sign in again".to_owned(),
            ));
        }
        if now < next_poll_at_ms {
            return Ok(ExternalStep::Pending {
                state: state.clone(),
                events: Vec::new(),
            });
        }
        let mut session = state.clone();
        let wait_secs = match self.poll_identity(&device_code).await? {
            DevicePoll::Pending => interval_secs,
            DevicePoll::SlowDown(extra) => {
                let widened = extra.unwrap_or_else(|| interval_secs.saturating_add(5));
                if let Some(object) = session.as_object_mut() {
                    object.insert("interval_secs".to_owned(), json!(widened));
                }
                widened
            }
            DevicePoll::Denied(_) => {
                return Ok(Self::failed_step("Meta login was denied".to_owned()));
            }
            DevicePoll::Expired => {
                return Ok(Self::failed_step(
                    "Meta device authorization expired; sign in again".to_owned(),
                ));
            }
            DevicePoll::Failed(reason) => return Ok(Self::failed_step(reason)),
            DevicePoll::Complete(identity_token) => {
                let (access, expires) = match self.mint_api_key(&identity_token).await {
                    Ok(minted) => minted,
                    Err(e) => return Ok(Self::failed_step(e.to_string())),
                };
                return Ok(ExternalStep::Completed {
                    credential: Credential::OAuth(OAuthCredential {
                        refresh: identity_token,
                        access,
                        expires,
                        is_subscription: true,
                        extra: Default::default(),
                    }),
                });
            }
        };
        if let Some(object) = session.as_object_mut() {
            object.insert(
                "next_poll_at_ms".to_owned(),
                json!(now_ms() + (wait_secs as i64) * 1000),
            );
        }
        Ok(ExternalStep::Pending {
            state: session,
            events: Vec::new(),
        })
    }

    async fn submit(&self, state: &ExternalState, _input: String) -> AuthResult<ExternalStep> {
        // The device flow never awaits pasted input; a stray submit is a bug.
        let _ = state;
        Ok(Self::failed_step(
            "this login flow takes no pasted input".to_owned(),
        ))
    }

    fn poll_interval(&self, state: &ExternalState) -> Duration {
        let secs = state
            .get("interval_secs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
        Duration::from_secs(secs.max(1))
    }
}

// ---------------------------------------------------------------------------
// Anthropic (Claude Pro/Max): PKCE browser / copy-code method choice.
// ---------------------------------------------------------------------------

/// OAuth client id of the Claude Code client.
pub const ANTHROPIC_CLIENT_ID: &str = "0d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// Authorization endpoint.
pub const ANTHROPIC_AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
/// Token endpoint; exchanges and refreshes take a JSON body.
pub const ANTHROPIC_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
/// Redirect target for the copy-code flow: the provider shows the code.
pub const ANTHROPIC_COPY_CODE_REDIRECT_URI: &str =
    "https://platform.claude.com/oauth/code/callback";
/// Requested scopes.
pub const ANTHROPIC_SCOPES: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
/// Preferred loopback port; falls back to an ephemeral port when busy.
const ANTHROPIC_BROWSER_PORT: u16 = 53692;
/// Loopback callback path.
const ANTHROPIC_CALLBACK_PATH: &str = "/callback";
/// Browser / copy-code method ids (match pi labels).
const METHOD_BROWSER: &str = "browser";
const METHOD_COPY_CODE: &str = "copy_code";
/// Expiry pulled back so a request never starts on a dying token.
const ANTHROPIC_EXPIRY_SKEW_MS: i64 = 5 * 60 * 1000;
/// How long the browser mode waits for the loopback callback.
const BROWSER_WAIT_SECS: u64 = 600;

/// Provider parameters: explicit declaration values win, pi defaults fill
/// the gaps so a bare `preset = "anthropic"` works.
#[derive(Debug, Clone)]
pub struct AnthropicParams {
    pub authorize_url: String,
    pub token_url: String,
    pub client_id: String,
    pub copy_code_redirect_uri: String,
    pub scopes: String,
}

impl AnthropicParams {
    pub fn from_declared(
        authorize_url: Option<String>,
        token_url: Option<String>,
        client_id: Option<String>,
        redirect_uri: Option<String>,
        scope: Option<String>,
    ) -> Self {
        Self {
            authorize_url: authorize_url
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| ANTHROPIC_AUTHORIZE_URL.to_owned()),
            token_url: token_url
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| ANTHROPIC_TOKEN_URL.to_owned()),
            client_id: client_id
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| ANTHROPIC_CLIENT_ID.to_owned()),
            copy_code_redirect_uri: redirect_uri
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| ANTHROPIC_COPY_CODE_REDIRECT_URI.to_owned()),
            scopes: scope
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| ANTHROPIC_SCOPES.to_owned()),
        }
    }
}

/// Outcome of reading one loopback callback request.
#[derive(Debug, Clone)]
enum CallbackOutcome {
    Code { code: String, state: String },
    ProviderError(String),
    MissingCode,
}

/// Shared slot between the loopback listener task and the poll step.
type CallbackSlot = Arc<Mutex<Option<CallbackOutcome>>>;

/// Pasted authorization input: a full callback URL, a `code#state` pair,
/// a query-shaped fragment, or a bare code.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PastedInput {
    Code { code: String, state: Option<String> },
    ProviderError(String),
    NotFound,
}

fn parse_pasted_input(input: &str) -> PastedInput {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return PastedInput::NotFound;
    }
    if trimmed.contains("://") {
        let Ok(url) = url::Url::parse(trimmed) else {
            return PastedInput::NotFound;
        };
        return pasted_from_pairs(
            url.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        );
    }
    if trimmed.contains('#') {
        let mut parts = trimmed.splitn(2, '#');
        let code = parts.next().unwrap_or_default().trim().to_owned();
        let state = parts
            .next()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        if code.is_empty() {
            return PastedInput::NotFound;
        }
        return PastedInput::Code { code, state };
    }
    if trimmed.contains("code=") {
        return pasted_from_pairs(
            url::form_urlencoded::parse(trimmed.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        );
    }
    PastedInput::Code {
        code: trimmed.to_owned(),
        state: None,
    }
}

fn pasted_from_pairs(pairs: Vec<(String, String)>) -> PastedInput {
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (key, value) in pairs {
        match key.as_str() {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => error = Some(value),
            _ => {}
        }
    }
    if let Some(error) = error {
        return PastedInput::ProviderError(error);
    }
    match code.filter(|v| !v.is_empty()) {
        Some(code) => PastedInput::Code {
            code,
            state: state.filter(|v| !v.is_empty()),
        },
        None => PastedInput::NotFound,
    }
}

/// Anthropic subscription login: the user picks browser or copy-code up
/// front (matching pi). Both modes exchange the code with a JSON body
/// carrying `state`; the browser mode serves one loopback callback while
/// copy-code accepts the pasted redirect URL or bare code.
#[derive(Debug, Clone)]
pub struct AnthropicAuth {
    display_name: String,
    login_label: Option<String>,
    params: AnthropicParams,
    callbacks: Arc<Mutex<HashMap<String, CallbackSlot>>>,
}

impl AnthropicAuth {
    pub fn new(
        display_name: impl Into<String>,
        login_label: Option<String>,
        params: AnthropicParams,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            login_label,
            params,
            callbacks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The provider surface for one declaration.
    pub fn provider_auth(
        display_name: impl Into<String>,
        login_label: Option<String>,
        params: AnthropicParams,
    ) -> ProviderAuth {
        ProviderAuth::oauth_only(Arc::new(Self::new(display_name, login_label, params)))
    }

    fn authorize_url(
        &self,
        challenge: &str,
        state: &str,
        redirect_uri: &str,
    ) -> AuthResult<String> {
        let mut url = url::Url::parse(&self.params.authorize_url)
            .map_err(|e| AuthError::Login(format!("authorize_url is not a URL: {e}")))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("code", "true");
            query.append_pair("client_id", &self.params.client_id);
            query.append_pair("response_type", "code");
            query.append_pair("redirect_uri", redirect_uri);
            query.append_pair("scope", &self.params.scopes);
            query.append_pair("code_challenge", challenge);
            query.append_pair("code_challenge_method", "S256");
            query.append_pair("state", state);
        }
        Ok(url.to_string())
    }

    async fn exchange_code(
        &self,
        code: &str,
        state: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> AuthResult<OAuthCredential> {
        let client = http_client()?;
        let (status, body) = read_json(
            client
                .post(&self.params.token_url)
                .header("Accept", "application/json")
                .json(&json!({
                    "grant_type": "authorization_code",
                    "client_id": self.params.client_id,
                    "code": code,
                    "state": state,
                    "redirect_uri": redirect_uri,
                    "code_verifier": verifier,
                }))
                .send()
                .await
                .map_err(|e| AuthError::Transport(format!("token exchange failed: {e}")))?,
        )
        .await?;
        if status != 200 {
            return Err(AuthError::Transport(format!(
                "token exchange failed (HTTP {status}): {body}"
            )));
        }
        Self::credential_from_token(&body, None)
    }

    fn credential_from_token(
        body: &Value,
        previous_refresh: Option<&str>,
    ) -> AuthResult<OAuthCredential> {
        let access = required_str(body, "access_token")
            .map_err(|e| AuthError::Transport(format!("invalid token response: {e}")))?;
        let refresh = body
            .get("refresh_token")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .or_else(|| previous_refresh.map(str::to_owned))
            .ok_or_else(|| {
                AuthError::Transport("token response carries no refresh_token".to_owned())
            })?;
        let expires_in = body
            .get("expires_in")
            .and_then(Value::as_i64)
            .unwrap_or(3600);
        Ok(OAuthCredential {
            refresh,
            access,
            expires: now_ms() + expires_in * 1000 - ANTHROPIC_EXPIRY_SKEW_MS,
            is_subscription: true,
            extra: Default::default(),
        })
    }

    fn failed_step(message: String) -> ExternalStep {
        ExternalStep::Failed {
            error: message,
            events: Vec::new(),
        }
    }

    /// Bind the loopback listener (preferred port, ephemeral fallback) and
    /// spawn one task serving exactly one callback request.
    async fn listen_once(&self, login_id: &str) -> AuthResult<(String, CallbackSlot)> {
        let listener =
            match tokio::net::TcpListener::bind(("127.0.0.1", ANTHROPIC_BROWSER_PORT)).await {
                Ok(listener) => listener,
                Err(_) => tokio::net::TcpListener::bind(("127.0.0.1", 0))
                    .await
                    .map_err(|e| AuthError::Login(format!("loopback listener failed: {e}")))?,
            };
        let port = listener
            .local_addr()
            .map(|addr| addr.port())
            .map_err(|e| AuthError::Login(format!("loopback port unknown: {e}")))?;
        let redirect_uri = format!("http://localhost:{port}{ANTHROPIC_CALLBACK_PATH}");
        let slot: CallbackSlot = Arc::new(Mutex::new(None));
        {
            let mut callbacks = self
                .callbacks
                .lock()
                .map_err(|e| AuthError::Login(format!("callback registry poisoned: {e}")))?;
            callbacks.insert(login_id.to_owned(), Arc::clone(&slot));
        }
        tokio::spawn(serve_single_callback(listener, Arc::clone(&slot)));
        Ok((redirect_uri, slot))
    }

    fn take_slot(&self, login_id: &str) -> Option<CallbackSlot> {
        self.callbacks
            .lock()
            .ok()
            .and_then(|mut callbacks| callbacks.remove(login_id))
    }
}

/// Accept one connection, record the callback outcome, answer plain text.
async fn serve_single_callback(listener: tokio::net::TcpListener, slot: CallbackSlot) {
    let accept =
        tokio::time::timeout(Duration::from_secs(BROWSER_WAIT_SECS), listener.accept()).await;
    let outcome = match accept {
        Err(_) => CallbackOutcome::ProviderError(
            "login timed out waiting for the browser callback".to_owned(),
        ),
        Ok(Err(e)) => CallbackOutcome::ProviderError(format!("callback accept failed: {e}")),
        Ok(Ok((mut stream, _))) => {
            let outcome = match read_http_request(&mut stream).await {
                Ok(request) => parse_callback_request(&request),
                Err(message) => CallbackOutcome::ProviderError(message),
            };
            respond_to_callback(&mut stream, &outcome).await;
            outcome
        }
    };
    if let Ok(mut slot) = slot.lock() {
        *slot = Some(outcome);
    }
}

/// Answer the callback request with a plain-text page.
async fn respond_to_callback(stream: &mut tokio::net::TcpStream, outcome: &CallbackOutcome) {
    let message: &str = match outcome {
        CallbackOutcome::Code { .. } => "Authorization complete. You can close this window.",
        CallbackOutcome::ProviderError(detail) => detail.as_str(),
        CallbackOutcome::MissingCode => "Callback carried no authorization code.",
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{message}",
        message.len()
    );
    use tokio::io::AsyncWriteExt;
    let _ = stream.write_all(response.as_bytes()).await;
}

/// Read until the end of HTTP headers or 16 KiB, whichever comes first.
async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 1024];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(30), stream.read(&mut chunk))
            .await
            .map_err(|_| "callback read timed out".to_owned())
            .and_then(|result| result.map_err(|e| format!("callback read failed: {e}")))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > 16 * 1024 {
            return Err("callback request too large".to_owned());
        }
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8(buffer).map_err(|_| "callback request is not UTF-8".to_owned())
}

/// Pull the authorization code (or provider error) out of the callback
/// request line.
fn parse_callback_request(request: &str) -> CallbackOutcome {
    let target = request.lines().next().unwrap_or_default();
    let mut parts = target.split_whitespace();
    let _method = parts.next();
    let path = parts.next().unwrap_or_default();
    let Ok(url) = url::Url::parse(&format!("http://localhost{path}")) else {
        return CallbackOutcome::MissingCode;
    };
    pasted_callback_to_outcome(
        url.query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect(),
    )
}

fn pasted_callback_to_outcome(pairs: Vec<(String, String)>) -> CallbackOutcome {
    match pasted_from_pairs(pairs) {
        PastedInput::Code { code, state } => CallbackOutcome::Code {
            code,
            state: state.unwrap_or_default(),
        },
        PastedInput::ProviderError(error) => CallbackOutcome::ProviderError(error),
        PastedInput::NotFound => CallbackOutcome::MissingCode,
    }
}

#[async_trait::async_trait]
impl OAuthAuth for AnthropicAuth {
    fn name(&self) -> String {
        self.display_name.clone()
    }

    fn is_subscription(&self) -> bool {
        true
    }

    fn login_label(&self) -> Option<String> {
        self.login_label.clone()
    }

    async fn begin_login(&self) -> AuthResult<LoginFlow> {
        Ok(LoginFlow::External(ExternalPending::new(
            json!({ "stage": "awaiting_method" }),
            Arc::new(self.clone()),
        )))
    }

    async fn refresh(&self, credential: &OAuthCredential) -> AuthResult<OAuthCredential> {
        let client = http_client()?;
        let (status, body) = read_json(
            client
                .post(&self.params.token_url)
                .header("Accept", "application/json")
                .json(&json!({
                    "grant_type": "refresh_token",
                    "client_id": self.params.client_id,
                    "refresh_token": credential.refresh,
                }))
                .send()
                .await
                .map_err(|e| AuthError::Refresh(format!("token refresh failed: {e}")))?,
        )
        .await
        .map_err(|e| AuthError::Refresh(e.to_string()))?;
        if status != 200 {
            return Err(AuthError::Refresh(format!(
                "token refresh failed (HTTP {status}): {body}"
            )));
        }
        Self::credential_from_token(&body, Some(&credential.refresh))
            .map_err(|e| AuthError::Refresh(e.to_string()))
    }

    async fn to_auth(&self, credential: &OAuthCredential) -> AuthResult<ModelAuth> {
        // Subscription calls require the OAuth beta flag; the wire version
        // itself is also set by the ANTHROPIC codec, but carrying both here
        // keeps single-credential resolution self-sufficient, matching the
        // reference plugin contract.
        let headers = [
            ("anthropic-version".to_owned(), "2023-06-01".to_owned()),
            ("anthropic-beta".to_owned(), "oauth-2025-04-20".to_owned()),
        ]
        .into_iter()
        .collect();
        Ok(ModelAuth {
            api_key: Some(credential.access.clone()),
            headers,
            is_subscription: true,
            source: Some("Anthropic (OAuth)".to_owned()),
            ..Default::default()
        })
    }
}

#[async_trait::async_trait]
impl ExternalFlowDriver for AnthropicAuth {
    async fn initial(&self, _state: &ExternalState) -> AuthResult<ExternalStep> {
        Ok(ExternalStep::AwaitingInput {
            state: json!({ "stage": "awaiting_method" }),
            prompt: AuthPrompt::Select {
                message: "Select Anthropic login method:".to_owned(),
                options: vec![
                    AuthPromptOption {
                        id: METHOD_BROWSER.to_owned(),
                        label: "Browser login (default)".to_owned(),
                        description: None,
                    },
                    AuthPromptOption {
                        id: METHOD_COPY_CODE.to_owned(),
                        label: "Copy code login (headless)".to_owned(),
                        description: None,
                    },
                ],
            },
            events: Vec::new(),
        })
    }

    async fn submit(&self, state: &ExternalState, input: String) -> AuthResult<ExternalStep> {
        let stage = state
            .get("stage")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match stage {
            "awaiting_method" => self.submit_method(input).await,
            "awaiting_code" => self.submit_code(state, input).await,
            _ => Ok(Self::failed_step(
                "unexpected login input for the current step".to_owned(),
            )),
        }
    }

    async fn poll(&self, state: &ExternalState) -> AuthResult<ExternalStep> {
        if state.get("stage").and_then(Value::as_str) != Some("awaiting_callback") {
            return Ok(Self::failed_step(
                "unexpected poll for the current step".to_owned(),
            ));
        }
        let login_id = state
            .get("login_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let verifier = state
            .get("verifier")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let redirect_uri = state
            .get("redirect_uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let outcome = self
            .callbacks
            .lock()
            .ok()
            .and_then(|callbacks| callbacks.get(login_id).cloned())
            .and_then(|slot| slot.lock().ok().and_then(|guard| guard.clone()));
        match outcome {
            None => Ok(ExternalStep::Pending {
                state: state.clone(),
                events: vec![AuthEvent::Progress {
                    message: "Waiting for the browser callback...".to_owned(),
                }],
            }),
            Some(CallbackOutcome::ProviderError(error)) => {
                self.take_slot(login_id);
                Ok(Self::failed_step(format!(
                    "the provider returned an error: {error}"
                )))
            }
            Some(CallbackOutcome::MissingCode) => {
                self.take_slot(login_id);
                Ok(Self::failed_step(
                    "callback carried no authorization code".to_owned(),
                ))
            }
            Some(CallbackOutcome::Code { code, state }) => {
                self.take_slot(login_id);
                if state != verifier {
                    return Ok(Self::failed_step(
                        "state mismatch: the callback does not belong to this login attempt"
                            .to_owned(),
                    ));
                }
                match self
                    .exchange_code(&code, &state, &verifier, &redirect_uri)
                    .await
                {
                    Ok(credential) => Ok(ExternalStep::Completed {
                        credential: Credential::OAuth(credential),
                    }),
                    Err(e) => Ok(Self::failed_step(e.to_string())),
                }
            }
        }
    }

    fn poll_interval(&self, _state: &ExternalState) -> Duration {
        Duration::from_secs(1)
    }
}

impl AnthropicAuth {
    async fn submit_method(&self, input: String) -> AuthResult<ExternalStep> {
        match input.trim() {
            METHOD_COPY_CODE => {
                let challenge = llm_auth::oauth::pkce::new_challenge();
                let url = self.authorize_url(
                    &challenge.challenge,
                    &challenge.verifier,
                    &self.params.copy_code_redirect_uri,
                )?;
                Ok(ExternalStep::AwaitingInput {
                    state: json!({
                        "stage": "awaiting_code",
                        "verifier": challenge.verifier,
                    }),
                    prompt: AuthPrompt::ManualCode {
                        message: "Paste the code Anthropic shows after you sign in:".to_owned(),
                        placeholder: Some("code#state".to_owned()),
                    },
                    events: vec![AuthEvent::AuthUrl {
                        url,
                        instructions: Some(
                            "Complete login in your browser, then copy the code Anthropic shows and paste it here."
                                .to_owned(),
                        ),
                    }],
                })
            }
            METHOD_BROWSER | "" => {
                let challenge = llm_auth::oauth::pkce::new_challenge();
                let login_id = challenge.verifier.clone();
                let (redirect_uri, _slot) = self.listen_once(&login_id).await?;
                let url =
                    self.authorize_url(&challenge.challenge, &challenge.verifier, &redirect_uri)?;
                Ok(ExternalStep::Pending {
                    state: json!({
                        "stage": "awaiting_callback",
                        "login_id": login_id,
                        "verifier": challenge.verifier,
                        "redirect_uri": redirect_uri,
                    }),
                    events: vec![AuthEvent::AuthUrl {
                        url,
                        instructions: Some(
                            "Complete login in your browser. If the browser is on another machine, restart with copy-code login instead."
                                .to_owned(),
                        ),
                    }],
                })
            }
            other => Ok(Self::failed_step(format!(
                "unknown Anthropic login method: {other}"
            ))),
        }
    }

    async fn submit_code(&self, state: &ExternalState, input: String) -> AuthResult<ExternalStep> {
        let verifier = state
            .get("verifier")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if verifier.is_empty() {
            return Ok(Self::failed_step(
                "login session carries no PKCE verifier".to_owned(),
            ));
        }
        let (code, pasted_state) = match parse_pasted_input(&input) {
            PastedInput::ProviderError(error) => {
                return Ok(Self::failed_step(format!(
                    "the provider returned an error: {error}"
                )));
            }
            PastedInput::NotFound => {
                return Ok(Self::failed_step(
                    "no authorization code found in the pasted input".to_owned(),
                ));
            }
            PastedInput::Code { code, state } => (code, state.unwrap_or(verifier.clone())),
        };
        if pasted_state != verifier {
            return Ok(Self::failed_step(
                "state mismatch: the pasted code does not belong to this login attempt".to_owned(),
            ));
        }
        match self
            .exchange_code(
                &code,
                &pasted_state,
                &verifier,
                &self.params.copy_code_redirect_uri,
            )
            .await
        {
            Ok(credential) => Ok(ExternalStep::Completed {
                credential: Credential::OAuth(credential),
            }),
            Err(e) => Ok(Self::failed_step(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn copilot_auth() -> GithubCopilotAuth {
        GithubCopilotAuth::new(
            "GitHub Copilot".to_owned(),
            Some("Sign in with GitHub".to_owned()),
            GithubCopilotParams::from_declared(None, None),
        )
    }

    #[test]
    fn copilot_params_default_to_pi_values() {
        let params = GithubCopilotParams::from_declared(None, None);
        assert_eq!(params.client_id, GITHUB_COPILOT_CLIENT_ID);
        assert_eq!(params.scope, GITHUB_COPILOT_SCOPE);
        let explicit =
            GithubCopilotParams::from_declared(Some("custom".to_owned()), Some(String::new()));
        assert_eq!(explicit.client_id, "custom");
        assert_eq!(explicit.scope, GITHUB_COPILOT_SCOPE);
    }

    #[test]
    fn copilot_urls_derive_from_domain() {
        let urls = copilot_urls("github.com");
        assert_eq!(urls.device_code_url, "https://github.com/login/device/code");
        assert_eq!(
            urls.copilot_token_url,
            "https://api.github.com/copilot_internal/v2/token"
        );
        let enterprise = copilot_urls("company.ghe.com");
        assert_eq!(
            enterprise.copilot_token_url,
            "https://api.company.ghe.com/copilot_internal/v2/token"
        );
    }

    #[test]
    fn normalize_domain_accepts_blank_and_enterprise_forms() {
        assert_eq!(normalize_domain("").expect("blank"), None);
        assert_eq!(normalize_domain("github.com").expect("default"), None);
        assert_eq!(
            normalize_domain("company.ghe.com").expect("host"),
            Some("company.ghe.com".to_owned())
        );
        assert_eq!(
            normalize_domain("https://company.ghe.com/login").expect("url"),
            Some("company.ghe.com".to_owned())
        );
        assert!(normalize_domain("not a domain!").is_err());
        assert!(normalize_domain("https://").is_err());
    }

    #[test]
    fn parse_device_code_validates_https_verification_uri() {
        let body = json!({
            "device_code": "dc",
            "user_code": "WDJB-MJHT",
            "verification_uri": "https://github.com/login/device",
        });
        let device = parse_device_code(&body).expect("device");
        assert_eq!(device.user_code, "WDJB-MJHT");
        let bad = json!({
            "device_code": "dc",
            "user_code": "WDJB-MJHT",
            "verification_uri": "javascript:alert(1)",
        });
        assert!(parse_device_code(&bad).is_err());
    }

    #[test]
    fn parse_device_poll_covers_pi_outcomes() {
        assert!(matches!(
            parse_device_poll(200, &json!({ "access_token": "at" })),
            DevicePoll::Complete(_)
        ));
        assert!(matches!(
            parse_device_poll(200, &json!({ "error": "authorization_pending" })),
            DevicePoll::Pending
        ));
        assert!(matches!(
            parse_device_poll(200, &json!({ "error": "slow_down", "interval": 10 })),
            DevicePoll::SlowDown(Some(10))
        ));
        assert!(matches!(
            parse_device_poll(200, &json!({ "error": "access_denied" })),
            DevicePoll::Denied(_)
        ));
        assert!(matches!(
            parse_device_poll(200, &json!({ "error": "expired_token" })),
            DevicePoll::Expired
        ));
        assert!(matches!(
            parse_device_poll(500, &json!({})),
            DevicePoll::Failed(_)
        ));
    }

    #[test]
    fn copilot_base_url_derives_from_proxy_claim() {
        assert_eq!(
            copilot_base_url("tid=1;exp=2;proxy-ep=proxy.individual.githubcopilot.com;other=3")
                .as_deref(),
            Some("https://api.individual.githubcopilot.com")
        );
        assert_eq!(copilot_base_url("tid=1;exp=2"), None);
        assert_eq!(copilot_base_url("proxy-ep=not a host!!"), None);
    }

    #[tokio::test]
    async fn copilot_initial_prompts_for_domain() {
        let step = copilot_auth().initial(&json!(null)).await.expect("initial");
        assert!(matches!(
            step,
            ExternalStep::AwaitingInput {
                prompt: AuthPrompt::Text { .. },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn copilot_submit_rejects_invalid_domain() {
        let session = json!({ "stage": "awaiting_domain" });
        let step = copilot_auth()
            .submit(&session, "not a domain!".to_owned())
            .await
            .expect("submit");
        assert!(matches!(step, ExternalStep::Failed { .. }));
    }

    #[tokio::test]
    async fn copilot_poll_rejects_unknown_stage() {
        let step = copilot_auth()
            .poll(&json!({ "stage": "awaiting_domain" }))
            .await
            .expect("poll");
        assert!(matches!(step, ExternalStep::Failed { .. }));
    }

    #[test]
    fn meta_params_default_to_pi_values() {
        let params = MetaMuseParams::from_declared(None, None, None);
        assert_eq!(params.client_id, META_CLIENT_ID);
        assert_eq!(
            params.device_authorization_url,
            META_DEVICE_AUTHORIZATION_URL
        );
    }

    #[test]
    fn anthropic_params_default_to_pi_values() {
        let params = AnthropicParams::from_declared(None, None, None, None, None);
        assert_eq!(params.client_id, ANTHROPIC_CLIENT_ID);
        assert_eq!(params.token_url, ANTHROPIC_TOKEN_URL);
        assert!(params.scopes.contains("user:inference"));
    }

    #[test]
    fn anthropic_authorize_url_carries_code_true_and_pkce() {
        let auth = AnthropicAuth::new(
            "Anthropic".to_owned(),
            None,
            AnthropicParams::from_declared(None, None, None, None, None),
        );
        let url = auth
            .authorize_url("challenge-1", "state-1", "https://example.test/cb")
            .expect("url");
        assert!(url.starts_with("https://claude.ai/oauth/authorize?"));
        assert!(url.contains("code=true"));
        assert!(url.contains("code_challenge=challenge-1"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state-1"));
    }

    #[test]
    fn parse_pasted_input_covers_pi_shapes() {
        assert!(matches!(
            parse_pasted_input("https://h.test/cb?code=abc&state=st"),
            PastedInput::Code { .. }
        ));
        assert!(matches!(
            parse_pasted_input("abc123#st1"),
            PastedInput::Code { .. }
        ));
        assert!(matches!(
            parse_pasted_input("  bare-code-42  "),
            PastedInput::Code { .. }
        ));
        assert!(matches!(
            parse_pasted_input("https://h.test/cb?error=access_denied"),
            PastedInput::ProviderError(_)
        ));
        assert!(matches!(
            parse_pasted_input("https://h.test/cb?foo=bar"),
            PastedInput::NotFound
        ));
        assert!(matches!(parse_pasted_input("   "), PastedInput::NotFound));
    }

    #[test]
    fn parse_callback_request_extracts_code_and_state() {
        let outcome = parse_callback_request(
            "GET /callback?code=the-code&state=the-state HTTP/1.1\r\nHost: x\r\n\r\n",
        );
        match outcome {
            CallbackOutcome::Code { code, state } => {
                assert_eq!(code, "the-code");
                assert_eq!(state, "the-state");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert!(matches!(
            parse_callback_request("GET /callback?error=denied HTTP/1.1\r\n\r\n"),
            CallbackOutcome::ProviderError(_)
        ));
    }

    #[tokio::test]
    async fn anthropic_initial_offers_method_choice() {
        let auth = AnthropicAuth::new(
            "Anthropic".to_owned(),
            None,
            AnthropicParams::from_declared(None, None, None, None, None),
        );
        let step = auth.initial(&json!(null)).await.expect("initial");
        match step {
            ExternalStep::AwaitingInput {
                prompt: AuthPrompt::Select { options, .. },
                ..
            } => {
                let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
                assert_eq!(ids, vec!["browser", "copy_code"]);
            }
            other => panic!("unexpected step: {other:?}"),
        }
    }

    #[tokio::test]
    async fn anthropic_to_auth_carries_oauth_beta_headers() {
        let auth = AnthropicAuth::new(
            "Anthropic".to_owned(),
            None,
            AnthropicParams::from_declared(None, None, None, None, None),
        );
        let credential = crate::llm_auth::credential::OAuthCredential {
            refresh: "rt".to_owned(),
            access: "at".to_owned(),
            expires: 0,
            is_subscription: true,
            extra: Default::default(),
        };
        let model_auth = auth.to_auth(&credential).await.expect("to_auth");
        assert_eq!(model_auth.api_key.as_deref(), Some("at"));
        assert_eq!(
            model_auth.headers.get("anthropic-beta").map(String::as_str),
            Some("oauth-2025-04-20")
        );
        assert_eq!(
            model_auth
                .headers
                .get("anthropic-version")
                .map(String::as_str),
            Some("2023-06-01")
        );
    }

    #[tokio::test]
    async fn anthropic_submit_rejects_unknown_method() {
        let auth = AnthropicAuth::new(
            "Anthropic".to_owned(),
            None,
            AnthropicParams::from_declared(None, None, None, None, None),
        );
        let step = auth
            .submit(&json!({ "stage": "awaiting_method" }), "sms".to_owned())
            .await
            .expect("submit");
        assert!(matches!(step, ExternalStep::Failed { .. }));
    }
}

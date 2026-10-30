//! Reference native plugin: Claude Pro/Max subscription login.
//!
//! CONTRACT SAMPLE, not the product login path. This crate exists to teach
//! the "provider bundle" integration path for a subscription-backed LLM
//! provider: the plugin contributes one `llm-auth` implementation (OAuth
//! 2.0 authorization code flow with PKCE, S256) and the matching
//! declarative provider definition in its manifest. The host owns the login
//! session, the credential store and the request pipeline; this crate only
//! speaks the synchronous JSON contract of [`PluginLlmAuth`].
//!
//! The production Claude Pro/Max login (plus the browser variant) ships as
//! the in-tree `anthropic` preset in `wf-llm`'s provider flows, which is a
//! strict superset of the paste-only flow implemented here. Keep this
//! crate's login logic frozen; extend the in-tree driver instead.
//!
//! Flow implemented here is the "paste" variant (`flow = "pkce_paste"`):
//! the authorize URL is rendered to the user, the provider redirects to a
//! page showing the code, and the user pastes the callback URL (or the bare
//! code) back. There is no local loopback server and no device-code
//! polling, so the plugin works in headless setups.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use wf_plugin_sdk::{
    export_plugin, AuthDescriptor, NativeRegistrar, PluginAuthEvent, PluginAuthNext,
    PluginAuthPrompt, PluginError, PluginLlmAuth, PluginLoginChallenge, PluginLoginStep,
    PluginManifest, PluginPermission, PluginProviderAuthRef, PluginResult, PluginType,
    WfNativePlugin,
};

/// Contribution name of the auth implementation. The manifest's provider
/// definition binds to this exact name, and the host dispatches ops as
/// `"llm-auth"` / `"example-claude-subscription/<op>"`.
const AUTH_NAME: &str = "example-claude-subscription";

/// The only OAuth flow variant this plugin implements.
const FLOW_PKCE_PASTE: &str = "pkce_paste";

/// Plugin id; must match `wf-plugin.toml` (the host cross-checks the
/// manifest exported over the C ABI against the on-disk one).
const PLUGIN_ID: &str = "example-llm-auth-claude-subscription";

/// Shared object the host loads; must match `wf-plugin.toml`.
const ENTRY_POINT: &str = "target/release/libexample_llm_auth_claude_subscription.so";

/// Subtracted from `expires_in` when computing the credential expiry so a
/// token near its boundary is refreshed one request early.
const EXPIRY_MARGIN_MS: i64 = 60_000;

/// Timeout for one token-endpoint call.
const HTTP_TIMEOUT_SECS: u64 = 30;

// ── declared auth parameters ─────────────────────────────────────────────

/// Declared auth parameters, read verbatim from the provider definition's
/// `auth.params` table. `login_start` receives them as `config`; they are
/// embedded into the login session and into the produced credential so
/// `refresh` can rebuild the token request without them.
#[derive(Debug)]
struct AuthParams {
    authorize_url: String,
    token_url: String,
    client_id: String,
    redirect_uri: String,
    scopes: String,
    flow: String,
}

impl AuthParams {
    /// Read the parameters the provider definition declared.
    fn from_config(config: &Value) -> PluginResult<Self> {
        let object = config
            .as_object()
            .ok_or_else(|| invalid("auth params must be a JSON object"))?;
        let authorize_url = required_str(object, "authorize_url")?;
        let token_url = required_str(object, "token_url")?;
        let client_id = required_str(object, "client_id")?;
        let redirect_uri = required_str(object, "redirect_uri")?;
        let scopes = object
            .get("scopes")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let flow = object
            .get("flow")
            .and_then(Value::as_str)
            .unwrap_or(FLOW_PKCE_PASTE)
            .to_owned();
        let params = Self {
            authorize_url,
            token_url,
            client_id,
            redirect_uri,
            scopes,
            flow,
        };
        if params.flow != FLOW_PKCE_PASTE {
            return Err(invalid(&format!(
                "unsupported auth flow '{}'; this plugin implements '{}'",
                params.flow, FLOW_PKCE_PASTE
            )));
        }
        Ok(params)
    }

    /// Recover the parameters a previous login embedded in the credential.
    fn from_credential(credential: &Value) -> PluginResult<Self> {
        let embedded = credential
            .get("extra")
            .and_then(|extra| extra.get("auth_params"))
            .ok_or_else(|| {
                invalid("credential carries no embedded auth params; sign in again")
            })?;
        Self::from_config(embedded)
    }

    /// Serialize back into the JSON shape the manifest declares.
    fn to_value(&self) -> Value {
        json!({
            "authorize_url": self.authorize_url,
            "token_url": self.token_url,
            "client_id": self.client_id,
            "redirect_uri": self.redirect_uri,
            "scopes": self.scopes,
            "flow": self.flow,
        })
    }
}

fn invalid(message: &str) -> PluginError {
    PluginError::InvalidContribution {
        plugin_id: PLUGIN_ID.to_owned(),
        message: message.to_owned(),
    }
}

fn required_str(object: &Map<String, Value>, key: &str) -> PluginResult<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid(&format!("auth params miss required key '{key}'")))
}

// ── PKCE helpers ─────────────────────────────────────────────────────────

/// URL-safe base64 without padding (RFC 7636 appendix A alphabet).
fn base64_url_nopad(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A fresh `(verifier, challenge)` pair; the challenge is
/// `BASE64URL(SHA256(verifier))` per RFC 7636.
fn pkce_pair() -> PluginResult<(String, String)> {
    let mut verifier_bytes = [0u8; 96];
    getrandom::getrandom(&mut verifier_bytes)
        .map_err(|e| invalid(&format!("pkce entropy unavailable: {e}")))?;
    let verifier = base64_url_nopad(&verifier_bytes);
    let digest = Sha256::digest(verifier.as_bytes());
    Ok((verifier, base64_url_nopad(&digest)))
}

/// Opaque anti-CSRF value echoed back by the provider on redirect.
fn random_state() -> PluginResult<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| invalid(&format!("state entropy unavailable: {e}")))?;
    Ok(base64_url_nopad(&bytes))
}

/// Build the authorize URL with the PKCE query parameters.
fn build_authorize_url(params: &AuthParams, challenge: &str, state: &str) -> PluginResult<String> {
    let mut url = url::Url::parse(&params.authorize_url)
        .map_err(|e| invalid(&format!("authorize_url is not a URL: {e}")))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("client_id", &params.client_id);
        query.append_pair("response_type", "code");
        query.append_pair("redirect_uri", &params.redirect_uri);
        query.append_pair("scope", &params.scopes);
        query.append_pair("state", state);
        query.append_pair("code_challenge", challenge);
        query.append_pair("code_challenge_method", "S256");
    }
    Ok(url.to_string())
}

// ── pasted callback input ────────────────────────────────────────────────

struct PastedCode {
    code: String,
    state: Option<String>,
}

enum PastedInput {
    Code(PastedCode),
    /// The provider redirected with an `error` parameter.
    ProviderError(String),
    /// No authorization code could be located in the input.
    NotFound,
}

/// Accept either the full callback URL or a bare authorization code.
/// Providers that answer in the URL fragment instead of the query are
/// handled too.
fn parse_pasted_input(input: &str) -> PastedInput {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return PastedInput::NotFound;
    }
    let Ok(url) = url::Url::parse(trimmed) else {
        // Not a URL at all: treat the whole input as the code.
        return PastedInput::Code(PastedCode {
            code: trimmed.to_owned(),
            state: None,
        });
    };
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if pairs.is_empty() {
        pairs = fragment_pairs(url.fragment().unwrap_or_default());
    }
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
    match code {
        Some(code) => PastedInput::Code(PastedCode { code, state }),
        None => PastedInput::NotFound,
    }
}

/// Split a `k=v&k=v` fragment; values are left percent-encoded because the
/// paste flow only needs to locate the code, not decode it.
fn fragment_pairs(fragment: &str) -> Vec<(String, String)> {
    fragment
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or_default();
            Some((key.to_owned(), value.to_owned()))
        })
        .collect()
}

// ── token endpoint ───────────────────────────────────────────────────────

/// The fields of a token-endpoint response this plugin consumes.
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    scope: Option<String>,
}

impl TokenResponse {
    fn from_json(body: &Value) -> PluginResult<Self> {
        let access_token = body
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| invalid("token response carries no access_token"))?;
        Ok(Self {
            access_token,
            refresh_token: body
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::to_owned),
            expires_in: body.get("expires_in").and_then(Value::as_i64),
            scope: body.get("scope").and_then(Value::as_str).map(str::to_owned),
        })
    }
}

/// POST one form-encoded token request and decode the JSON response.
/// Protocol-level failures are reported as [`PastedInput`]-style messages
/// the login step can surface; transport failures hard-error.
fn token_request(params: &AuthParams, http: &reqwest::blocking::Client, body: &Value) -> PluginResult<TokenResponse> {
    let form: Vec<(String, String)> = body
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| {
                    let rendered = match value {
                        Value::String(text) => text.clone(),
                        other => other.to_string(),
                    };
                    (key.clone(), rendered)
                })
                .collect()
        })
        .unwrap_or_default();

    let response = http
        .post(&params.token_url)
        .form(&form)
        .send()
        .map_err(|e| invalid(&format!("token request failed: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|e| invalid(&format!("token response unreadable: {e}")))?;
    if !status.is_success() {
        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|body| {
                let description = body
                    .get("error_description")
                    .and_then(Value::as_str)
                    .or_else(|| body.get("error").and_then(Value::as_str));
                description.map(str::to_owned)
            })
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(invalid(&format!("token endpoint rejected the request: {detail}")));
    }
    let body: Value = serde_json::from_str(&text)
        .map_err(|e| invalid(&format!("token response is not JSON: {e}")))?;
    TokenResponse::from_json(&body)
}

fn now_unix_millis() -> PluginResult<i64> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| invalid(&format!("system clock before unix epoch: {e}")))?;
    Ok(i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
}

/// Build the credential JSON the host persists:
/// `{access, refresh, expires, is_subscription, extra}` with `expires` as a
/// Unix-millisecond timestamp and the auth parameters embedded under
/// `extra.auth_params` for later refreshes.
fn credential_from_token(
    token: &TokenResponse,
    params: &AuthParams,
    previous: Option<&Value>,
) -> PluginResult<Value> {
    let refresh_token = token
        .refresh_token
        .clone()
        .or_else(|| {
            previous
                .and_then(|credential| credential.get("refresh"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .ok_or_else(|| {
            invalid("token response carries no refresh_token; the host credential requires one")
        })?;
    let now = now_unix_millis()?;
    // A missing `expires_in` yields 0: the host then treats the credential
    // as always expired and refreshes it before the first request.
    let expires = token
        .expires_in
        .map(|seconds| now + seconds.saturating_mul(1000) - EXPIRY_MARGIN_MS)
        .unwrap_or(0);

    let mut extra = Map::new();
    extra.insert("auth_params".to_owned(), params.to_value());
    if let Some(scope) = &token.scope {
        extra.insert("scope".to_owned(), json!(scope));
    }
    if let Some(previous_extra) = previous
        .and_then(|credential| credential.get("extra"))
        .and_then(Value::as_object)
    {
        for (key, value) in previous_extra {
            if key != "auth_params" && key != "scope" {
                extra.insert(key.clone(), value.clone());
            }
        }
    }

    Ok(json!({
        "access": token.access_token,
        "refresh": refresh_token,
        "expires": expires,
        "is_subscription": true,
        "extra": Value::Object(extra),
    }))
}

// ── the auth implementation ──────────────────────────────────────────────

struct ClaudeSubscriptionAuth {
    /// The blocking client owns a dedicated runtime thread, so it is safe
    /// to drive from the host's synchronous dispatch calls regardless of
    /// the async runtime the host itself runs on.
    http: OnceLock<reqwest::blocking::Client>,
}

impl ClaudeSubscriptionAuth {
    fn new() -> Self {
        Self {
            http: OnceLock::new(),
        }
    }

    fn http(&self) -> PluginResult<&reqwest::blocking::Client> {
        if let Some(client) = self.http.get() {
            return Ok(client);
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
            .build()
            .map_err(|e| invalid(&format!("http client build: {e}")))?;
        // A racing thread may have installed its own client first; either
        // instance is equivalent, so the loser's is dropped.
        let _ = self.http.set(client);
        self.http
            .get()
            .ok_or_else(|| invalid("http client unavailable"))
    }

    fn failed_step(error: String) -> PluginLoginStep {
        PluginLoginStep {
            events: vec![PluginAuthEvent::Info {
                message: format!("Claude subscription sign-in failed: {error}"),
            }],
            session: Value::Null,
            next: PluginAuthNext::Failed { error },
        }
    }
}

impl PluginLlmAuth for ClaudeSubscriptionAuth {
    fn describe(&self) -> PluginResult<AuthDescriptor> {
        Ok(AuthDescriptor {
            name: "Claude (Pro/Max Subscription)".to_owned(),
            is_subscription: true,
            login_label: Some("Sign in with Claude Pro/Max".to_owned()),
        })
    }

    fn login_start(&self, config: Value) -> PluginResult<PluginLoginChallenge> {
        let params = AuthParams::from_config(&config)?;
        let (verifier, challenge) = pkce_pair()?;
        let state = random_state()?;
        let authorize_url = build_authorize_url(&params, &challenge, &state)?;
        Ok(PluginLoginChallenge {
            events: vec![
                PluginAuthEvent::Info {
                    message: "Opening the Claude subscription sign-in page.".to_owned(),
                },
                PluginAuthEvent::AuthUrl {
                    url: authorize_url,
                    instructions: Some(
                        "Approve access, then paste the full callback URL (or just the code) back here."
                            .to_owned(),
                    ),
                },
            ],
            session: json!({
                "stage": "awaiting_code",
                "params": params.to_value(),
                "verifier": verifier,
                "state": state,
            }),
            next: PluginAuthNext::AwaitingInput {
                prompt: PluginAuthPrompt::ManualCode {
                    message: "Paste the callback URL after approving access".to_owned(),
                    placeholder: Some(
                        "https://console.anthropic.com/oauth/code/callback?code=...&state=..."
                            .to_owned(),
                    ),
                },
            },
        })
    }

    fn login_poll(&self, session: Value) -> PluginResult<PluginLoginStep> {
        // The paste flow has no server-side polling: the host only waits on
        // the user's pasted code, so every poll is a progress heartbeat.
        Ok(PluginLoginStep {
            events: vec![PluginAuthEvent::Progress {
                message: "Waiting for the pasted callback code...".to_owned(),
            }],
            session,
            next: PluginAuthNext::Pending,
        })
    }

    fn login_submit(&self, session: Value, input: String) -> PluginResult<PluginLoginStep> {
        let params = AuthParams::from_config(&session.get("params").cloned().unwrap_or(Value::Null))?;
        let expected_state = session
            .get("state")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("login session carries no state"))?
            .to_owned();
        let verifier = session
            .get("verifier")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("login session carries no pkce verifier"))?
            .to_owned();

        let pasted = match parse_pasted_input(&input) {
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
            PastedInput::Code(pasted) => pasted,
        };
        if let Some(state) = pasted.state.as_deref() {
            if state != expected_state {
                return Ok(Self::failed_step(
                    "state mismatch: the pasted callback does not belong to this login attempt"
                        .to_owned(),
                ));
            }
        }

        let http = self.http()?;
        let body = json!({
            "grant_type": "authorization_code",
            "code": pasted.code,
            "client_id": params.client_id,
            "redirect_uri": params.redirect_uri,
            "code_verifier": verifier,
        });
        let token = match token_request(&params, http, &body) {
            Ok(token) => token,
            Err(error) => return Ok(Self::failed_step(error.to_string())),
        };
        let credential = credential_from_token(&token, &params, None)?;
        Ok(PluginLoginStep {
            events: vec![PluginAuthEvent::Info {
                message: "Claude subscription connected.".to_owned(),
            }],
            session: Value::Null,
            next: PluginAuthNext::Completed { credential },
        })
    }

    fn refresh(&self, credential: Value) -> PluginResult<Value> {
        let params = AuthParams::from_credential(&credential)?;
        let refresh_token = credential
            .get("refresh")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("credential carries no refresh token"))?
            .to_owned();
        let http = self.http()?;
        let body = json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": params.client_id,
            "scope": params.scopes,
        });
        let token = token_request(&params, http, &body)?;
        credential_from_token(&token, &params, Some(&credential))
    }

    fn resolve(&self, credential: Value) -> PluginResult<Value> {
        let access = credential
            .get("access")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("credential carries no access token"))?;
        // Subscription access to the Anthropic API requires the OAuth beta
        // flag; the exact header set depends on the account plan and must be
        // validated against a real subscription (open question Q5).
        let headers = json!({
            "anthropic-version": "2023-06-01",
            "anthropic-beta": "oauth-2025-04-20",
        });
        Ok(json!({
            "api_key": access,
            "headers": headers,
            "is_subscription": true,
            "source": format!("plugin:{AUTH_NAME}"),
        }))
    }
}

// ── native plugin wiring ─────────────────────────────────────────────────

struct ClaudeSubscriptionPlugin {
    auth: ClaudeSubscriptionAuth,
}

impl WfNativePlugin for ClaudeSubscriptionPlugin {
    fn new() -> Self {
        Self {
            auth: ClaudeSubscriptionAuth::new(),
        }
    }

    fn manifest() -> PluginManifest {
        // Must stay in sync with wf-plugin.toml; the snapshot test asserts
        // the two agree.
        let mut auth_params: HashMap<String, Value> = HashMap::new();
        auth_params.insert(
            "authorize_url".to_owned(),
            json!("https://claude.ai/oauth/authorize"),
        );
        auth_params.insert(
            "token_url".to_owned(),
            json!("https://console.anthropic.com/v1/oauth/token"),
        );
        auth_params.insert(
            "client_id".to_owned(),
            json!("9d1c250a-e61b-44d9-88ed-5944d1962f5e"),
        );
        auth_params.insert(
            "redirect_uri".to_owned(),
            json!("https://console.anthropic.com/oauth/code/callback"),
        );
        auth_params.insert("scopes".to_owned(), json!("user:inference"));
        auth_params.insert("flow".to_owned(), json!(FLOW_PKCE_PASTE));

        PluginManifest {
            id: PLUGIN_ID.to_owned(),
            version: "0.1.0".to_owned(),
            name: Some("Example: Claude Subscription Auth".to_owned()),
            description: Some(
                "Sample plugin: Claude Pro/Max subscription login through the host's llm-auth contribution point (PKCE paste flow). Contract teaching sample, not for production use."
                    .to_owned(),
            ),
            plugin_type: Some(PluginType::Native),
            entry_point: ENTRY_POINT.to_owned(),
            versioned_entry_points: HashMap::new(),
            dependencies: HashMap::new(),
            optional_dependencies: HashMap::new(),
            contributions: vec![AUTH_NAME.to_owned()],
            permissions: vec![PluginPermission::LlmAuth, PluginPermission::Network],
            config_schema: None,
            config: None,
            hooks: None,
            tools: Vec::new(),
            tool_descriptions: Vec::new(),
            llm_providers: vec![wf_plugin_sdk::PluginLlmProviderDefinition {
                id: AUTH_NAME.to_owned(),
                format: "ANTHROPIC".to_owned(),
                name: Some("Claude (Pro/Max Subscription)".to_owned()),
                description: Some(
                    "Anthropic Messages API reached through a Claude Pro/Max subscription."
                        .to_owned(),
                ),
                base_url: Some("https://api.anthropic.com".to_owned()),
                auth_type: Some("bearer".to_owned()),
                default_headers: None,
                api_version: None,
                model_discovery: None,
                metadata: None,
                proxy: None,
                no_proxy: None,
                rate_limit: None,
                compat: None,
                identity: None,
                transport: None,
                env: None,
                auth: Some(PluginProviderAuthRef {
                    provider: AUTH_NAME.to_owned(),
                    method: Some("oauth".to_owned()),
                    is_subscription: Some(true),
                    params: Some(auth_params),
                }),
            }],
            wasm: None,
            lua: None,
        }
    }

    fn dispatch(&self, kind: &str, name: &str, input: &Value) -> PluginResult<Value> {
        if kind != "llm-auth" {
            return Err(invalid(&format!(
                "unexpected handler type '{kind}'; this plugin only serves 'llm-auth'"
            )));
        }
        let op = name
            .strip_prefix(AUTH_NAME)
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or_else(|| {
                invalid(&format!(
                    "handler '{name}' is not owned by auth contribution '{AUTH_NAME}'"
                ))
            })?;
        let session = |key: &str| input.get(key).cloned().unwrap_or(Value::Null);
        match op {
            "describe" => to_json(&self.auth.describe()?),
            "login_start" => to_json(&self.auth.login_start(session("config"))?),
            "login_poll" => to_json(&self.auth.login_poll(session("session"))?),
            "login_submit" => {
                let pasted = input.get("input").and_then(Value::as_str).unwrap_or("");
                to_json(&self.auth.login_submit(session("session"), pasted.to_owned())?)
            }
            "refresh" => to_json(&self.auth.refresh(session("credential"))?),
            "resolve" => to_json(&self.auth.resolve(session("credential"))?),
            other => Err(invalid(&format!("unknown auth op '{other}'"))),
        }
    }

    fn register(registrar: &NativeRegistrar) {
        registrar.register_llm_auth(AUTH_NAME);
    }
}

fn to_json(value: &impl serde::Serialize) -> PluginResult<Value> {
    serde_json::to_value(value)
        .map_err(|e| invalid(&format!("response serialization: {e}")))
}

export_plugin!(ClaudeSubscriptionPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn auth() -> ClaudeSubscriptionAuth {
        ClaudeSubscriptionAuth::new()
    }

    fn test_params() -> AuthParams {
        AuthParams::from_config(&test_config()).expect("params")
    }

    fn test_config() -> Value {
        json!({
            "authorize_url": "https://provider.test/authorize",
            "token_url": "https://provider.test/token",
            "client_id": "client-123",
            "redirect_uri": "https://provider.test/callback",
            "scopes": "user:inference",
            "flow": "pkce_paste",
        })
    }

    fn challenge() -> PluginLoginChallenge {
        auth().login_start(test_config()).expect("login_start")
    }

    #[test]
    fn manifest_snapshot_matches_toml_file() {
        let rendered = toml::to_string_pretty(&ClaudeSubscriptionPlugin::manifest())
            .expect("manifest serializes to toml");
        let exported: toml::Value =
            toml::from_str(&rendered).expect("rendered manifest parses as toml");
        let snapshot: toml::Value =
            toml::from_str(include_str!("../wf-plugin.toml")).expect("manifest toml parses");
        assert_eq!(exported, snapshot);
    }

    #[test]
    fn describe_reports_subscription_metadata() {
        let descriptor = auth().describe().expect("describe");
        assert_eq!(descriptor.name, "Claude (Pro/Max Subscription)");
        assert!(descriptor.is_subscription);
        assert_eq!(
            descriptor.login_label.as_deref(),
            Some("Sign in with Claude Pro/Max")
        );
    }

    #[test]
    fn pkce_pair_uses_s256_challenge_of_verifier() {
        let (verifier, challenge) = pkce_pair().expect("pkce pair");
        assert_eq!(verifier.len(), 128);
        let expected = base64_url_nopad(&Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, expected);
        assert!(!challenge.contains('='));
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
    }

    #[test]
    fn parse_pasted_input_handles_url_fragment_and_bare_code() {
        match parse_pasted_input("https://provider.test/callback?code=abc123&state=st1") {
            PastedInput::Code(pasted) => {
                assert_eq!(pasted.code, "abc123");
                assert_eq!(pasted.state.as_deref(), Some("st1"));
            }
            _ => panic!("expected code"),
        }
        match parse_pasted_input("  bare-code-42  ") {
            PastedInput::Code(pasted) => {
                assert_eq!(pasted.code, "bare-code-42");
                assert!(pasted.state.is_none());
            }
            _ => panic!("expected code"),
        }
        assert!(matches!(
            parse_pasted_input("https://provider.test/callback#code=frag&state=st2"),
            PastedInput::Code(_)
        ));
        assert!(matches!(
            parse_pasted_input("https://provider.test/callback?error=access_denied"),
            PastedInput::ProviderError(_)
        ));
        assert!(matches!(
            parse_pasted_input("https://provider.test/callback?foo=bar"),
            PastedInput::NotFound
        ));
        assert!(matches!(parse_pasted_input("   "), PastedInput::NotFound));
    }

    #[test]
    fn login_start_builds_authorize_url_and_awaits_manual_code() {
        let challenge = challenge();
        assert_eq!(challenge.session["stage"], "awaiting_code");
        assert!(!challenge.session["verifier"].as_str().expect("verifier").is_empty());
        let state = challenge.session["state"].as_str().expect("state");
        assert!(!state.is_empty());
        assert_eq!(challenge.session["params"]["token_url"], "https://provider.test/token");

        let PluginAuthNext::AwaitingInput { prompt } = &challenge.next else {
            panic!("expected awaiting input");
        };
        assert!(matches!(prompt, PluginAuthPrompt::ManualCode { .. }));

        let auth_url = challenge
            .events
            .iter()
            .find_map(|event| match event {
                PluginAuthEvent::AuthUrl { url, .. } => Some(url.clone()),
                _ => None,
            })
            .expect("auth url event");
        assert!(auth_url.starts_with("https://provider.test/authorize?"));
        assert!(auth_url.contains("response_type=code"));
        assert!(auth_url.contains("client_id=client-123"));
        assert!(auth_url.contains("code_challenge_method=S256"));
        assert!(auth_url.contains(&format!("state={state}")));
        let verifier = challenge.session["verifier"].as_str().expect("verifier");
        let expected_challenge = base64_url_nopad(&Sha256::digest(verifier.as_bytes()));
        assert!(auth_url.contains(&format!("code_challenge={expected_challenge}")));
    }

    #[test]
    fn login_start_rejects_unsupported_flow() {
        let mut config = test_config();
        config["flow"] = json!("device_code");
        let error = auth().login_start(config).expect_err("must reject");
        assert!(error.to_string().contains("device_code"));
    }

    #[test]
    fn login_submit_rejects_state_mismatch() {
        let session = challenge().session;
        let step = auth()
            .login_submit(
                session,
                "https://provider.test/callback?code=abc&state=not-the-one".to_owned(),
            )
            .expect("submit");
        let PluginAuthNext::Failed { error } = step.next else {
            panic!("expected failed step");
        };
        assert!(error.contains("state mismatch"), "{error}");
    }

    #[test]
    fn login_submit_rejects_input_without_code() {
        let session = challenge().session;
        let step = auth()
            .login_submit(session, "https://provider.test/callback?foo=bar".to_owned())
            .expect("submit");
        assert!(matches!(step.next, PluginAuthNext::Failed { .. }));
    }

    #[test]
    fn credential_from_token_carries_params_and_expiry() {
        let params = test_params();
        let token = TokenResponse {
            access_token: "at-1".to_owned(),
            refresh_token: Some("rt-1".to_owned()),
            expires_in: Some(3600),
            scope: Some("user:inference".to_owned()),
        };
        let credential = credential_from_token(&token, &params, None).expect("credential");
        assert_eq!(credential["access"], "at-1");
        assert_eq!(credential["refresh"], "rt-1");
        assert_eq!(credential["is_subscription"], true);
        let now = now_unix_millis().expect("now");
        let expires = credential["expires"].as_i64().expect("expires");
        assert!(expires > now + 3_500_000 && expires <= now + 3_600_000);
        assert_eq!(
            credential["extra"]["auth_params"]["token_url"],
            "https://provider.test/token"
        );
        assert_eq!(credential["extra"]["scope"], "user:inference");
    }

    #[test]
    fn credential_from_token_keeps_previous_refresh_token() {
        let params = test_params();
        let token = TokenResponse {
            access_token: "at-2".to_owned(),
            refresh_token: None,
            expires_in: None,
            scope: None,
        };
        let previous = json!({ "refresh": "rt-old", "extra": { "note": "keep-me" } });
        let credential =
            credential_from_token(&token, &params, Some(&previous)).expect("credential");
        assert_eq!(credential["refresh"], "rt-old");
        assert_eq!(credential["expires"], 0);
        assert_eq!(credential["extra"]["note"], "keep-me");
    }

    #[test]
    fn credential_without_refresh_token_is_rejected() {
        let params = test_params();
        let token = TokenResponse {
            access_token: "at-3".to_owned(),
            refresh_token: None,
            expires_in: Some(60),
            scope: None,
        };
        let error = credential_from_token(&token, &params, None).expect_err("must reject");
        assert!(error.to_string().contains("refresh_token"));
    }

    #[test]
    fn refresh_reads_params_from_credential_extra() {
        let params = test_params();
        let credential = json!({
            "access": "at-1",
            "refresh": "rt-1",
            "expires": 1,
            "is_subscription": true,
            "extra": { "auth_params": params.to_value() },
        });
        let recovered = AuthParams::from_credential(&credential).expect("params");
        assert_eq!(recovered.token_url, "https://provider.test/token");
        assert_eq!(recovered.client_id, "client-123");

        let bare = json!({ "access": "at-1", "refresh": "rt-1" });
        let error = AuthParams::from_credential(&bare).expect_err("must reject");
        assert!(error.to_string().contains("auth params"));
    }

    #[test]
    fn resolve_returns_subscription_model_auth() {
        let credential = json!({ "access": "at-1", "refresh": "rt-1", "expires": 1 });
        let resolved = auth().resolve(credential).expect("resolve");
        assert_eq!(resolved["api_key"], "at-1");
        assert_eq!(resolved["is_subscription"], true);
        assert_eq!(resolved["source"], "plugin:example-claude-subscription");
        assert_eq!(resolved["headers"]["anthropic-version"], "2023-06-01");
        assert_eq!(resolved["headers"]["anthropic-beta"], "oauth-2025-04-20");
    }

    #[test]
    fn dispatch_routes_registered_ops() {
        let plugin = ClaudeSubscriptionPlugin::new();
        let described: Value = plugin
            .dispatch("llm-auth", "example-claude-subscription/describe", &json!({}))
            .expect("describe");
        assert_eq!(described["name"], "Claude (Pro/Max Subscription)");

        let started: Value = plugin
            .dispatch(
                "llm-auth",
                "example-claude-subscription/login_start",
                &json!({ "config": test_config() }),
            )
            .expect("login_start");
        assert_eq!(started["next"]["next"], "awaiting_input");
        assert_eq!(started["session"]["params"]["flow"], "pkce_paste");

        let polled: Value = plugin
            .dispatch(
                "llm-auth",
                "example-claude-subscription/login_poll",
                &json!({ "session": started["session"].clone() }),
            )
            .expect("login_poll");
        assert_eq!(polled["next"]["next"], "pending");

        let error = plugin
            .dispatch("llm-auth", "example-claude-subscription/nope", &json!({}))
            .expect_err("unknown op");
        assert!(error.to_string().contains("unknown auth op"));
        let error = plugin
            .dispatch("llm-codec", "example-claude-subscription/describe", &json!({}))
            .expect_err("wrong kind");
        assert!(error.to_string().contains("unexpected handler type"));
    }
}

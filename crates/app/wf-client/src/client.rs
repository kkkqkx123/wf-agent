//! Hand-written client wrapper over the progenitor-generated API.
//!
//! The generated `Client` exposes one builder per operation (487 ops); this
//! wrapper adds connection setup (`x-api-key` header injection), the
//! `ApiEnvelope<T>` unwrap shared by every endpoint, a JSON escape hatch for
//! management commands, and startup health probing.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{map_progenitor, ClientError, ClientResult};
use crate::generated;

/// Default local server address when neither `--remote` nor `WF_REMOTE` is set.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:3000";

/// Built-in agent definition id used when no `--agent` is given. Mirrors the
/// server default for empty ids.
pub const DEFAULT_AGENT: &str = "@standard/main";

/// Model profile id used when no `--model` is given.
pub const DEFAULT_MODEL: &str = "default";

/// Connection settings for one `wf-server` instance.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL, e.g. `http://127.0.0.1:3000` (no trailing slash).
    pub base_url: String,
    /// Value for the `x-api-key` header; also read from `WF_API_KEY`.
    pub api_key: Option<String>,
}

impl ClientConfig {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: None,
        }
    }

    pub fn with_api_key(mut self, key: Option<String>) -> Self {
        if let Some(key) = key {
            if !key.trim().is_empty() {
                self.api_key = Some(key);
            }
        }
        self
    }

    /// Resolve from explicit flags first, then the environment.
    pub fn resolve(remote: Option<String>, api_key: Option<String>) -> Self {
        let base_url = remote
            .filter(|u| !u.trim().is_empty())
            .or_else(env_remote_url)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let key = api_key.filter(|k| !k.trim().is_empty()).or_else(env_api_key);
        Self::new(base_url).with_api_key(key)
    }
}

/// Shared HTTP client for `wf-server`.
#[derive(Debug, Clone)]
pub struct WfClient {
    config: ClientConfig,
    http: reqwest::Client,
    generated: generated::Client,
}

impl WfClient {
    /// Build from an explicit config.
    pub fn new(config: ClientConfig) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(key) = &config.api_key {
            if let Ok(value) = reqwest::header::HeaderValue::from_str(key) {
                headers.insert("x-api-key", value);
            }
        }
        let http = reqwest::Client::builder()
            .user_agent("wf-client/0.1")
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let generated = generated::Client::new_with_client(&config.base_url, http.clone());
        Self {
            config,
            http,
            generated,
        }
    }

    /// Resolve connection from CLI flags / environment.
    pub fn from_remote(remote: Option<String>, api_key: Option<String>) -> Self {
        Self::new(ClientConfig::resolve(remote, api_key))
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Access the fully-typed progenitor client for new call sites.
    pub fn generated(&self) -> &generated::Client {
        &self.generated
    }

    /// Underlying `reqwest` client (used by the SSE layer).
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// API key header value, when configured.
    pub fn api_key(&self) -> Option<&str> {
        self.config.api_key.as_deref()
    }

    /// Probe `GET /health`; maps auth failures explicitly so callers can hint
    /// at `WF_API_KEY` instead of printing a generic connection error.
    pub async fn check_health(&self) -> ClientResult<serde_json::Value> {
        let url = format!("{}/health", self.config.base_url);
        let resp = self.http.get(&url).send().await.map_err(|e| {
            ClientError::Http(format!("cannot reach server at {}: {e}", self.config.base_url))
        })?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ClientError::NotFound(url));
        }
        if !resp.status().is_success() {
            return Err(ClientError::Http(format!(
                "health check failed with HTTP {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ClientError::Invalid(format!("health body: {e}")))?;
        unwrap_envelope(&body)
    }

    /// Typed GET that unwraps the standard `ApiEnvelope<T>` shell.
    pub async fn get<T>(&self, path: &str) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.config.base_url, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        read_envelope::<T>(resp).await
    }

    /// Typed POST with JSON body that unwraps `ApiEnvelope<T>`.
    pub async fn post<B, T>(&self, path: &str, body: &B) -> ClientResult<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.config.base_url, path);
        let resp = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        read_envelope::<T>(resp).await
    }

    /// Typed PUT / DELETE / PATCH helpers for management commands.
    pub async fn put<B, T>(&self, path: &str, body: &B) -> ClientResult<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.config.base_url, path);
        let resp = self
            .http
            .put(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        read_envelope::<T>(resp).await
    }

    pub async fn delete<T>(&self, path: &str) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.config.base_url, path);
        let resp = self
            .http
            .delete(&url)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        read_envelope::<T>(resp).await
    }

    /// Untyped GET returning the unwrapped `data` payload as JSON.
    pub async fn get_json(&self, path: &str) -> ClientResult<serde_json::Value> {
        self.get(path).await
    }

    /// Untyped POST returning the unwrapped `data` payload as JSON.
    pub async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> ClientResult<serde_json::Value> {
        self.post(path, body).await
    }

    pub async fn put_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> ClientResult<serde_json::Value> {
        self.put(path, body).await
    }

    pub async fn delete_json(&self, path: &str) -> ClientResult<serde_json::Value> {
        self.delete(path).await
    }
}

async fn read_envelope<T>(resp: reqwest::Response) -> ClientResult<T>
where
    T: DeserializeOwned,
{
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ClientError::Unauthorized);
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ClientError::NotFound(status.to_string()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ClientError::Invalid(format!("response body: {e}")))?;
    if !status.is_success() {
        return Err(envelope_error(&body, status.as_u16()));
    }
    unwrap_envelope(&body)
}

/// Split an `ApiEnvelope` JSON body into its `data` payload.
pub fn unwrap_envelope<T>(body: &serde_json::Value) -> ClientResult<T>
where
    T: DeserializeOwned,
{
    if body.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return Err(envelope_error(body, 0));
    }
    match body.get("data") {
        None => Err(ClientError::Invalid("missing data field".to_string())),
        Some(serde_json::Value::Null) => Err(ClientError::Invalid("empty data".to_string())),
        Some(data) => serde_json::from_value(data.clone())
            .map_err(|e| ClientError::Invalid(format!("data decode: {e}"))),
    }
}

fn envelope_error(body: &serde_json::Value, status: u16) -> ClientError {
    let code = body
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or("REMOTE_ERROR");
    let message = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("request failed");
    if status == 401 {
        return ClientError::Unauthorized;
    }
    ClientError::Remote {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn env_remote_url() -> Option<String> {
    std::env::var("WF_REMOTE")
        .ok()
        .filter(|v| !v.trim().is_empty())
}

fn env_api_key() -> Option<String> {
    std::env::var("WF_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// Adapt a typed progenitor call into [`ClientError`].
pub fn into_client_error<E>(err: progenitor_client::Error<E>) -> ClientError
where
    E: std::fmt::Debug,
{
    map_progenitor(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_explicit_remote() {
        let cfg = ClientConfig::resolve(
            Some("http://example:9999".to_string()),
            Some("k".to_string()),
        );
        assert_eq!(cfg.base_url, "http://example:9999");
        assert_eq!(cfg.api_key.as_deref(), Some("k"));
    }

    #[test]
    fn unwrap_envelope_extracts_data() {
        let body = serde_json::json!({"success": true, "data": {"a": 1}, "error": null});
        let data: serde_json::Value = unwrap_envelope(&body).unwrap();
        assert_eq!(data, serde_json::json!({"a": 1}));
    }

    #[test]
    fn unwrap_envelope_maps_remote_error() {
        let body = serde_json::json!({
            "success": false,
            "data": null,
            "error": {"code": "NOT_FOUND", "message": "missing"}
        });
        let err = unwrap_envelope::<serde_json::Value>(&body).unwrap_err();
        assert!(matches!(err, ClientError::Remote { .. }));
    }
}

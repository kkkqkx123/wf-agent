//! Hand-written client wrapper over the progenitor-generated API.
//!
//! Strong-type-first: new call sites use [`WfClient::generated`] or the
//! typed helpers below (named paths, typed bodies, typed envelopes).
//! The `*_json` methods are the narrow JSON escape hatch. Retained uses
//! fall into six categories; new uses require a retention reason or they
//! do not merge:
//!
//! - dynamic observation queries (filters, aggregates, exports, searches);
//! - metrics export and storage health/diagnose passthrough;
//! - entity payloads (messages, tasks, variables, skills, interactions);
//! - plugin registries (tools, scripts: externally defined content);
//! - terminal streams and file downloads (non-envelope transports);
//! - forward-compatibility and debug probes.

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{from_status_body, map_progenitor, map_typed_error, ClientError, ClientResult};
use crate::generated;

/// Default local server address when neither `--remote` nor `WF_REMOTE` is set.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:3000";

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
        let key = api_key
            .filter(|k| !k.trim().is_empty())
            .or_else(env_api_key);
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
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(15))
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
    pub fn generated(&self) -> &crate::GeneratedClient {
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
        self.get("/health").await.map_err(|e| match e {
            ClientError::Http(msg) => ClientError::Http(format!(
                "cannot reach server at {}: {msg}",
                self.config.base_url
            )),
            other => other,
        })
    }

    /// Typed health probe returning the named view.
    pub async fn health_typed(&self) -> ClientResult<crate::models::HealthView> {
        let envelope = self
            .generated
            .handle_health()
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed server info probe.
    pub async fn info_typed(&self) -> ClientResult<crate::models::InfoView> {
        let envelope = self
            .generated
            .handle_info()
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed single agent-loop run (`POST /api/v1/agent-loops/{id}/run`).
    pub async fn run_agent_loop(
        &self,
        id: &str,
        body: &crate::models::RunAgentLoopBody,
    ) -> ClientResult<crate::models::AgentRunView> {
        let envelope = self
            .generated
            .handle_run_loop(id, body)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed single workflow execute (`POST /api/v1/workflows/{id}/execute`).
    pub async fn execute_workflow(
        &self,
        id: &str,
        body: &crate::models::ExecuteBody,
    ) -> ClientResult<crate::models::ExecuteView> {
        let envelope = self
            .generated
            .handle_execute_workflow(id, body)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed background workflow execute
    /// (`POST /api/v1/workflows/{id}/execute/background`).
    pub async fn execute_workflow_background(
        &self,
        id: &str,
        body: &crate::models::ExecuteBody,
    ) -> ClientResult<crate::models::BackgroundExecuteView> {
        let envelope = self
            .generated
            .handle_execute_background(id, body)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed workflow lock acquire (`POST /api/v1/workflows/{id}/lock/acquire`).
    pub async fn acquire_workflow_lock(
        &self,
        id: &str,
        body: &crate::models::AcquireLockBody,
    ) -> ClientResult<crate::models::LockView> {
        let envelope = self
            .generated
            .handle_acquire_lock(id, body)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed workflow lock heartbeat (`POST /api/v1/workflows/{id}/lock/heartbeat`).
    pub async fn heartbeat_workflow_lock(
        &self,
        id: &str,
        body: &crate::models::OwnerBody,
    ) -> ClientResult<crate::models::LockView> {
        let envelope = self
            .generated
            .handle_heartbeat_lock(id, body)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed workflow lock release (`POST /api/v1/workflows/{id}/lock/release`).
    pub async fn release_workflow_lock(
        &self,
        id: &str,
        body: &crate::models::OwnerBody,
    ) -> ClientResult<Option<crate::models::LockView>> {
        let envelope = self
            .generated
            .handle_release_lock(id, body)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed workflow lock query (`GET /api/v1/workflows/{id}/lock`).
    pub async fn get_workflow_lock(
        &self,
        id: &str,
    ) -> ClientResult<Option<crate::models::LockView>> {
        let envelope = self
            .generated
            .handle_get_lock(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed execution pause (`POST /api/v1/executions/{id}/pause`).
    /// Response `data` stays generic, so the path and envelope are typed
    /// while the payload keeps passing through as JSON.
    pub async fn pause_execution(&self, id: &str) -> ClientResult<serde_json::Value> {
        let envelope = self
            .generated
            .handle_pause(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed execution resume (`POST /api/v1/executions/{id}/resume`).
    pub async fn resume_execution(&self, id: &str) -> ClientResult<serde_json::Value> {
        let envelope = self
            .generated
            .handle_resume(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed execution cancel (`POST /api/v1/executions/{id}/cancel`).
    pub async fn cancel_execution(&self, id: &str) -> ClientResult<serde_json::Value> {
        let envelope = self
            .generated
            .handle_cancel(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed execution status (`GET /api/v1/executions/{id}/status`).
    pub async fn execution_status(&self, id: &str) -> ClientResult<String> {
        let envelope = self
            .generated
            .handle_status(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed agent-loop pause (`POST /api/v1/agent-loops/{id}/pause`).
    pub async fn pause_agent_loop(&self, id: &str) -> ClientResult<serde_json::Value> {
        let envelope = self
            .generated
            .handle_pause_loop(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed agent-loop resume (`POST /api/v1/agent-loops/{id}/resume`).
    pub async fn resume_agent_loop(&self, id: &str) -> ClientResult<serde_json::Value> {
        let envelope = self
            .generated
            .handle_resume_loop(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed agent-loop cancel (`POST /api/v1/agent-loops/{id}/cancel`).
    pub async fn cancel_agent_loop(&self, id: &str) -> ClientResult<serde_json::Value> {
        let envelope = self
            .generated
            .handle_cancel_loop(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed agent-loop status (`GET /api/v1/agent-loops/{id}/status`).
    pub async fn agent_loop_status(&self, id: &str) -> ClientResult<String> {
        let envelope = self
            .generated
            .handle_loop_status(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed agent-loop summary (`GET /api/v1/agent-loops/{id}/summary`).
    pub async fn agent_loop_summary(&self, id: &str) -> ClientResult<serde_json::Value> {
        let envelope = self
            .generated
            .handle_loop_summary(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed execution detail (`GET /api/v1/executions/{id}`).
    /// Response `data` is a plain string on this route, so the path and
    /// envelope stay typed while the payload passes through as JSON.
    pub async fn execution_detail(&self, id: &str) -> ClientResult<serde_json::Value> {
        let envelope = self
            .generated
            .handle_get_execution(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed execution hierarchy (`GET /api/v1/executions/{id}/hierarchy`).
    pub async fn execution_hierarchy(
        &self,
        id: &str,
    ) -> ClientResult<crate::models::HierarchyDoc> {
        let envelope = self
            .generated
            .get_executions_id_hierarchy(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed execution subtree page (`GET /api/v1/executions/{id}/subtree`).
    pub async fn execution_subtree(
        &self,
        id: &str,
        cursor: Option<&str>,
        limit: Option<i64>,
    ) -> ClientResult<crate::models::SubtreeDoc> {
        let envelope = self
            .generated
            .get_executions_id_subtree(id, cursor, limit)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed execution history (`GET /api/v1/executions/{id}/history`).
    pub async fn execution_history(
        &self,
        id: &str,
        include: Option<&str>,
    ) -> ClientResult<crate::models::HistoryDoc> {
        let envelope = self
            .generated
            .get_executions_id_history(id, include)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed checkpoint chain analysis for one execution
    /// (`GET /api/v1/executions/{id}/checkpoints/chain`).
    pub async fn execution_checkpoints_chain(
        &self,
        id: &str,
    ) -> ClientResult<crate::models::CappedChainDoc> {
        let envelope = self
            .generated
            .get_executions_id_checkpoints_chain(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Typed checkpoint chain for one agent loop
    /// (`GET /api/v1/agent-loops/{id}/checkpoints/chain`).
    /// The loop route returns a capped item view rather than the execution
    /// analysis shape, so the unwrapped data type differs intentionally.
    pub async fn agent_loop_checkpoints_chain(
        &self,
        id: &str,
    ) -> ClientResult<crate::models::ApiEnvelopeCappedViewCheckpointDocData> {
        let envelope = self
            .generated
            .get_agent_loops_id_checkpoints_chain(id)
            .await
            .map_err(map_typed_error)?;
        unwrap_generated_envelope(envelope.into_inner())
    }

    /// Single-send core for the JSON escape hatch: every verb funnels here
    /// so envelope handling stays in one place.
    async fn send<T>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.config.base_url, path);
        let mut request = self.http.request(method, &url);
        if let Some(body) = body {
            request = request.json(body);
        }
        let resp = request
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        read_envelope::<T>(resp).await
    }

    /// Typed GET that unwraps the standard `ApiEnvelope<T>` shell.
    /// Private: outside callers use the documented `get_json` escape hatch
    /// or a typed helper above so the retention reason stays explicit.
    async fn get<T>(&self, path: &str) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.send(reqwest::Method::GET, path, None).await
    }

    /// Typed POST with JSON body that unwraps `ApiEnvelope<T>`.
    /// Private: outside callers use `post_json` or a typed helper.
    async fn post<B, T>(&self, path: &str, body: &B) -> ClientResult<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let body = serde_json::to_value(body)
            .map_err(|e| ClientError::Invalid(format!("request encode: {e}")))?;
        self.send(reqwest::Method::POST, path, Some(&body)).await
    }

    /// Typed PUT helper for management commands.
    /// Private: outside callers use `put_json`.
    async fn put<B, T>(&self, path: &str, body: &B) -> ClientResult<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let body = serde_json::to_value(body)
            .map_err(|e| ClientError::Invalid(format!("request encode: {e}")))?;
        self.send(reqwest::Method::PUT, path, Some(&body)).await
    }

    /// Private: outside callers use `delete_json`.
    async fn delete<T>(&self, path: &str) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.send(reqwest::Method::DELETE, path, None).await
    }

    /// JSON escape hatch: same unwrap as above, kept for endpoints whose
    /// `data` is still generic `Value`, dynamic queries, and probes.
    pub async fn get_json<T>(&self, path: &str) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.get(path).await
    }

    /// JSON escape hatch for POST; prefer a typed helper for new call sites.
    pub async fn post_json<B, T>(&self, path: &str, body: &B) -> ClientResult<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.post(path, body).await
    }

    pub async fn put_json<B, T>(&self, path: &str, body: &B) -> ClientResult<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.put(path, body).await
    }

    pub async fn delete_json<T>(&self, path: &str) -> ClientResult<T>
    where
        T: DeserializeOwned,
    {
        self.delete(path).await
    }
}

/// Unwrap a progenitor envelope by round-tripping through JSON so every
/// generated success type shares one `success`/`data` check.
pub fn unwrap_generated_envelope<T, D>(envelope: T) -> ClientResult<D>
where
    T: Serialize,
    D: DeserializeOwned,
{
    let value = serde_json::to_value(&envelope)
        .map_err(|e| ClientError::Invalid(format!("envelope encode: {e}")))?;
    unwrap_envelope(&value)
}

async fn read_envelope<T>(resp: reqwest::Response) -> ClientResult<T>
where
    T: DeserializeOwned,
{
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ClientError::Invalid(format!("response body: {e}")))?;
    if status == 401 {
        return Err(ClientError::Unauthorized);
    }
    if !(200..300).contains(&status) {
        return Err(from_status_body(status, &body));
    }
    unwrap_envelope(&body)
}

/// Split an `ApiEnvelope` JSON body into its `data` payload.
pub fn unwrap_envelope<T>(body: &serde_json::Value) -> ClientResult<T>
where
    T: DeserializeOwned,
{
    if body.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return Err(from_status_body(0, body));
    }
    match body.get("data") {
        None => Err(ClientError::Invalid("missing data field".to_string())),
        // `Null` decodes into `Option<T>` as `None`; non-optional targets
        // fail with a decode error below, which is the correct signal.
        Some(data) => serde_json::from_value(data.clone())
            .map_err(|e| ClientError::Invalid(format!("data decode: {e}"))),
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

/// Adapt a typed progenitor call carrying `ErrorResponse`, preserving the
/// server-sent code and message.
pub fn into_typed_client_error(
    err: progenitor_client::Error<crate::models::ErrorResponse>,
) -> ClientError {
    map_typed_error(err)
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

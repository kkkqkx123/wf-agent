//! How external services are reached: plain HTTP calls plus supervised
//! local service processes (managed transport).
//!
//! A sidecar is a service binary the runtime starts, probes for readiness
//! and supervises: unexpected exits restart with backoff up to a bound,
//! runtime shutdown stops the process. The wire contract stays plain HTTP;
//! supervision only owns the process lifecycle, never the protocol.
//!
//! Every call passes one circuit breaker per origin, so an unresponsive
//! service degrades to immediate rejections instead of one full timeout per
//! call. Failures are typed ([`ServiceError`]) so callers can tell an
//! unreachable service from a rejected request.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod breaker;
pub mod sidecar;

/// Header carrying the per-call request id, so one logical call can be
/// correlated between this process and the service logs.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

pub use breaker::{breaker_for, origin_of, BreakerPhase, BreakerSnapshot, CircuitBreaker};
pub use sidecar::{
    start_sidecar, RunningSidecar, SidecarPhase, SidecarSpec, SidecarStatus, SidecarStatusHandle,
};

/// How an external service is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TransportMode {
    /// Connect to an independently deployed service address.
    #[default]
    External,
    /// Let the runtime start and supervise a local service process.
    Managed,
}

/// Transport description shared by every external-service adapter: call
/// timeout, service address vs runtime-supervised local process. Domain
/// policy (budgets, defaults) stays in the adapter that owns it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceTransport {
    /// Overall timeout for one service call in milliseconds.
    pub timeout_ms: u64,
    /// External address vs runtime-supervised local process.
    pub transport_mode: TransportMode,
    /// Service base URL. Required when enabled with external transport.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Service binary launched in managed mode.
    pub managed_binary: String,
    /// Loopback port for the managed process (`0` picks an ephemeral one).
    pub managed_port: u16,
    /// Bound for managed process startup including readiness probing.
    pub managed_startup_timeout_ms: u64,
    /// Restarts allowed after unexpected exits before the process is left
    /// down.
    pub managed_max_restarts: u32,
    /// Environment variable holding the bearer token. Declarations carry no
    /// secrets: the value is read once when the service is built and never
    /// enters logs, status snapshots or error text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token_env: Option<String>,
    /// Log level handed to the managed process through its environment.
    pub managed_log_level: String,
}

impl Default for ServiceTransport {
    fn default() -> Self {
        Self {
            timeout_ms: 60_000,
            transport_mode: TransportMode::External,
            base_url: None,
            managed_binary: "cce-server".to_string(),
            managed_port: 0,
            managed_startup_timeout_ms: 15_000,
            managed_max_restarts: 3,
            auth_token_env: None,
            managed_log_level: "info".to_string(),
        }
    }
}

impl ServiceTransport {
    /// Configured base URL with surrounding whitespace and trailing slashes
    /// removed. Only meaningful for external transport.
    pub fn normalized_base_url(&self) -> Option<String> {
        self.base_url.as_deref().and_then(|url| {
            let trimmed = url.trim().trim_end_matches('/');
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
    }

    /// Bearer token declared by this transport, read from the environment
    /// variable name it names. `Ok(None)` means no credential is declared.
    pub fn resolve_auth_token(&self) -> Result<Option<String>, String> {
        let Some(name) = self.auth_token_env.as_deref().map(str::trim) else {
            return Ok(None);
        };
        if name.is_empty() {
            return Ok(None);
        }
        let value = std::env::var(name).map_err(|_| {
            format!(
                "code-context credential is not resolvable: environment variable {name} is not set"
            )
        })?;
        let value = value.trim();
        if value.is_empty() {
            return Err(format!(
                "code-context credential is not resolvable: environment variable {name} is empty"
            ));
        }
        Ok(Some(value.to_string()))
    }
}

/// Which subsystem a call needs. Availability is decided per scope: folding
/// and navigation only need the process plus its metadata store, while
/// retrieval additionally needs the embedder or the keyword index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallScope {
    /// File folding: parser only.
    Fold,
    /// Symbol navigation: metadata store only.
    Navigation,
    /// BM25 keyword search: keyword index.
    KeywordSearch,
    /// Hybrid semantic search: query embedding.
    HybridSearch,
    /// Operational endpoints (project, index, dead letters): the service
    /// process must answer, no subsystem is required. Maintenance must work
    /// while an index runs or after it failed, which is exactly when the
    /// retrieval scopes are unavailable.
    Operations,
}

impl CallScope {
    /// Whether a health report covers this scope.
    pub fn covered_by(self, report: &HealthReport) -> bool {
        if !report.reachable {
            return false;
        }
        match self {
            CallScope::Fold | CallScope::Navigation | CallScope::Operations => true,
            CallScope::KeywordSearch => report.bm25,
            CallScope::HybridSearch => report.embedding,
        }
    }

    /// Human-readable capability name for rejection messages.
    pub fn requirement(self) -> &'static str {
        match self {
            CallScope::Fold | CallScope::Navigation => "metadata service",
            CallScope::KeywordSearch => "keyword index",
            CallScope::HybridSearch => "embedding service",
            CallScope::Operations => "service process",
        }
    }
}

/// Service health as reported by `/api/health`.
///
/// `reachable` records that the endpoint answered; the subsystem flags
/// record what actually works. A service can be reachable and still unable
/// to embed, which is exactly the state folding still tolerates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HealthReport {
    /// The endpoint answered with a decodable health payload.
    pub reachable: bool,
    /// The service reported every subsystem healthy.
    pub healthy: bool,
    /// Vector backend reachable.
    pub vectors: bool,
    /// Keyword index reachable.
    pub bm25: bool,
    /// Embedding provider reachable.
    pub embedding: bool,
}

impl HealthReport {
    /// Unreachable report: nothing answered.
    pub fn unreachable() -> Self {
        Self {
            reachable: false,
            healthy: false,
            vectors: false,
            bm25: false,
            embedding: false,
        }
    }
}

/// Failure class of one service call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceErrorKind {
    /// The service did not answer: transport failure, timeout, server-side
    /// failure, or an open breaker.
    Unavailable,
    /// The service answered and refused the request (client error or an
    /// in-band error payload).
    Rejected,
    /// The response was undecodable, which means a protocol/version
    /// mismatch rather than an availability problem.
    Protocol,
    /// The service answered and refused the credential or its project scope.
    /// Never a breaker input: retrying cannot fix a wrong token.
    Credential,
}

/// One failed service call. Unavailable failures feed the breaker; rejected
/// and protocol failures do not, because retrying or breaking would hide a
/// configuration problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceError {
    pub kind: ServiceErrorKind,
    pub reason: String,
}

impl ServiceError {
    /// Unavailable failure (transport, timeout, server-side, open breaker).
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            kind: ServiceErrorKind::Unavailable,
            reason: reason.into(),
        }
    }

    /// Rejected failure (client error or in-band error payload).
    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            kind: ServiceErrorKind::Rejected,
            reason: reason.into(),
        }
    }

    /// Protocol failure (undecodable response).
    pub fn protocol(reason: impl Into<String>) -> Self {
        Self {
            kind: ServiceErrorKind::Protocol,
            reason: reason.into(),
        }
    }

    /// Credential failure (refused token or project scope).
    pub fn credential(reason: impl Into<String>) -> Self {
        Self {
            kind: ServiceErrorKind::Credential,
            reason: reason.into(),
        }
    }

    /// Whether the service refused the credential rather than failing.
    pub fn is_credential(&self) -> bool {
        self.kind == ServiceErrorKind::Credential
    }

    /// Whether the service was unreachable rather than refusing the request.
    pub fn is_unavailable(&self) -> bool {
        self.kind == ServiceErrorKind::Unavailable
    }

    /// Flatten into the reason string.
    pub fn into_reason(self) -> String {
        self.reason
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl From<ServiceError> for String {
    fn from(error: ServiceError) -> Self {
        error.reason
    }
}

/// Build a JSON-capable HTTP client with a client-level timeout and, when a
/// credential is declared, its default authorization header. Authentication
/// lives on the client so every channel (fold, retrieval, navigation,
/// health probe) carries it without per-call plumbing. Failures here mean the
/// integration itself is misconfigured, never the service.
pub fn http_client(timeout_ms: u64, auth_token: Option<&str>) -> Result<reqwest::Client, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = auth_token {
        let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| "code-context credential is not a valid header value".to_string())?;
        value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
    reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms.max(1)))
        .default_headers(headers)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

/// Client-level timeout of the shared service client. Every call carries its
/// own outer timeout, so this only bounds a request nobody timed out.
const SHARED_CLIENT_TIMEOUT_MS: u64 = 1_800_000;

fn credential_cell() -> &'static std::sync::RwLock<Option<String>> {
    static CELL: std::sync::OnceLock<std::sync::RwLock<Option<String>>> =
        std::sync::OnceLock::new();
    CELL.get_or_init(|| std::sync::RwLock::new(None))
}

/// Credential the cached client was built with, plus the client itself.
type SharedClientCell = std::sync::Mutex<Option<(Option<String>, reqwest::Client)>>;

fn shared_client_cell() -> &'static SharedClientCell {
    static CELL: std::sync::OnceLock<SharedClientCell> = std::sync::OnceLock::new();
    CELL.get_or_init(|| std::sync::Mutex::new(None))
}

/// Register the process-wide service credential.
///
/// The credential is a process-level transport fact, exactly like the breaker
/// registry: channels that only hold a baked base URL (the compression fold
/// chain) must authenticate without a credential ever entering their config.
/// The value never leaves this module.
pub fn register_service_credential(token: Option<&str>) {
    let mut credential = match credential_cell().write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *credential = token.map(str::to_string);
}

/// Shared service client used by channels that only know a base URL. Built
/// once per registered credential.
pub fn service_client() -> Result<reqwest::Client, String> {
    let token = match credential_cell().read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    let mut cell = match shared_client_cell().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some((cached_token, client)) = cell.as_ref() {
        if *cached_token == token {
            return Ok(client.clone());
        }
    }
    let client = http_client(SHARED_CLIENT_TIMEOUT_MS, token.as_deref())?;
    *cell = Some((token, client.clone()));
    Ok(client)
}

/// Allocate a request id for one service call. Process-local, monotonic and
/// unique enough to line up client logs with whatever the service records.
pub fn next_request_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("wf-{millis}-{:x}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// Status codes that mean "the service is not serving right now" rather than
/// "the request is wrong".
fn status_is_unavailable(status: reqwest::StatusCode) -> bool {
    status.is_server_error()
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
}

/// Send one request under a single overall timeout and decode the JSON
/// answer. The origin's breaker is consulted first; unavailable failures are
/// recorded on it, so a dead service stops costing a full timeout per call.
async fn send_json(
    request: reqwest::RequestBuilder,
    url: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value, ServiceError> {
    let breaker = breaker_for(url);
    breaker.admit().map_err(ServiceError::unavailable)?;
    let request_id = next_request_id();
    let request = request.header(REQUEST_ID_HEADER, &request_id);
    tracing::debug!(request_id = %request_id, url, "code-context request");
    let send = request.send();
    let response = match tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), send).await
    {
        Err(_) => {
            breaker.record_failure();
            tracing::warn!(request_id = %request_id, url, timeout_ms, "code-context request timed out");
            return Err(ServiceError::unavailable(format!(
                "request to {url} timed out after {timeout_ms} ms (request {request_id})"
            )));
        }
        Ok(Err(e)) => {
            breaker.record_failure();
            tracing::warn!(request_id = %request_id, url, error = %e, "code-context request failed");
            return Err(ServiceError::unavailable(format!(
                "request to {url} failed: {e} (request {request_id})"
            )));
        }
        Ok(Ok(response)) => response,
    };
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let reason: String = body.chars().take(300).collect();
        tracing::warn!(request_id = %request_id, url, %status, "code-context request rejected");
        if status_is_unavailable(status) {
            breaker.record_failure();
            return Err(ServiceError::unavailable(format!(
                "request to {url} rejected with status {status}: {reason} (request {request_id})"
            )));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ServiceError::credential(format!(
                "code-context credential or project scope was refused (status {status}) at {url}; check the configured token and its project scope"
            )));
        }
        return Err(ServiceError::rejected(format!(
            "request to {url} rejected with status {status}: {reason} (request {request_id})"
        )));
    }
    match response.json::<serde_json::Value>().await {
        Ok(value) => {
            breaker.record_success();
            tracing::debug!(request_id = %request_id, url, "code-context response");
            Ok(value)
        }
        Err(e) => Err(ServiceError::protocol(format!(
            "response from {url} was not decodable: {e} (request {request_id})"
        ))),
    }
}

/// POST a JSON body and decode the JSON answer with a single overall
/// timeout.
pub async fn post_json(
    client: &reqwest::Client,
    url: &str,
    timeout_ms: u64,
    body: &serde_json::Value,
) -> Result<serde_json::Value, ServiceError> {
    send_json(client.post(url).json(body), url, timeout_ms).await
}

/// GET a JSON answer with a single overall timeout, under the same breaker
/// and request-id rules as [`post_json`].
pub async fn get_json(
    client: &reqwest::Client,
    url: &str,
    timeout_ms: u64,
) -> Result<serde_json::Value, ServiceError> {
    send_json(client.get(url), url, timeout_ms).await
}

/// Parse a `/api/health` payload. Absent subsystem entries inherit the
/// overall verdict, which keeps the parser usable across service versions
/// that report fewer subsystems.
pub fn parse_health(payload: &serde_json::Value) -> Result<HealthReport, ServiceError> {
    let healthy = payload
        .get("healthy")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ServiceError::protocol("health payload carries no 'healthy' flag"))?;
    let subsystem = |key: &str| {
        payload
            .get(key)
            .and_then(|v| v.get("reachable"))
            .and_then(|v| v.as_bool())
            .unwrap_or(healthy)
    };
    Ok(HealthReport {
        reachable: true,
        healthy,
        vectors: subsystem("qdrant"),
        bm25: subsystem("bm25"),
        embedding: subsystem("embedding"),
    })
}

/// Probe `/api/health` and parse the subsystem verdict.
///
/// Probing deliberately bypasses the breaker: the probe is the truth source
/// that closes it again, so it must never be refused by the state it
/// observes. A successful probe resets the breaker, a failed one records the
/// failure like any other call.
pub async fn probe_health(
    client: &reqwest::Client,
    endpoint: &str,
    timeout_ms: u64,
) -> Result<HealthReport, ServiceError> {
    let url = format!("{}/api/health", endpoint.trim_end_matches('/'));
    let breaker = breaker_for(endpoint);
    let request_id = next_request_id();
    let send = client
        .get(&url)
        .header(REQUEST_ID_HEADER, &request_id)
        .send();
    let response = match tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), send).await
    {
        Err(_) => {
            breaker.record_failure();
            return Ok(HealthReport::unreachable());
        }
        Ok(Err(_)) => {
            breaker.record_failure();
            return Ok(HealthReport::unreachable());
        }
        Ok(Ok(response)) => response,
    };
    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ServiceError::credential(format!(
                "code-context health probe was refused with status {} at {url}; check the configured token and its project scope",
                response.status()
            )));
        }
        breaker.record_failure();
        return Ok(HealthReport::unreachable());
    }
    match response.json::<serde_json::Value>().await {
        Ok(payload) => match parse_health(&payload) {
            Ok(report) => {
                breaker.record_success();
                Ok(report)
            }
            Err(error) => Err(error),
        },
        Err(e) => Err(ServiceError::protocol(format!(
            "health response from {url} was not decodable: {e}"
        ))),
    }
}

/// Pick a free loopback port by binding an ephemeral listener. Best-effort:
/// the port is released before the child binds it, so this suits loopback
/// single-host use only.
pub fn pick_loopback_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_payload_parses_subsystem_verdicts() {
        let report = parse_health(&serde_json::json!({
            "healthy": false,
            "qdrant": { "reachable": true },
            "bm25": { "reachable": true },
            "embedding": { "reachable": false }
        }))
        .expect("parse health");
        assert!(report.reachable);
        assert!(!report.healthy);
        assert!(report.vectors);
        assert!(!report.embedding);
        assert!(CallScope::Fold.covered_by(&report));
        assert!(CallScope::KeywordSearch.covered_by(&report));
        assert!(!CallScope::HybridSearch.covered_by(&report));

        let unreachable = parse_health(&serde_json::json!({
            "healthy": true,
            "qdrant": { "reachable": false },
            "bm25": { "reachable": false },
            "embedding": { "reachable": true }
        }))
        .expect("parse health");
        assert!(!unreachable.vectors);
        assert!(CallScope::HybridSearch.covered_by(&unreachable));
        assert!(!CallScope::KeywordSearch.covered_by(&unreachable));
    }

    #[test]
    fn health_payload_without_verdict_is_a_protocol_error() {
        let error = parse_health(&serde_json::json!({ "status": "ok" })).unwrap_err();
        assert_eq!(error.kind, ServiceErrorKind::Protocol);
    }

    #[test]
    fn base_url_normalization_strips_empty_and_trailing_slashes() {
        let transport = ServiceTransport {
            base_url: Some(" http://127.0.0.1:9000/ ".into()),
            ..Default::default()
        };
        assert_eq!(
            transport.normalized_base_url().as_deref(),
            Some("http://127.0.0.1:9000")
        );
        let blank = ServiceTransport {
            base_url: Some("   ".into()),
            ..Default::default()
        };
        assert!(blank.normalized_base_url().is_none());
    }
}

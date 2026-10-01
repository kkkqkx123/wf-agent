//! How external services are reached: plain HTTP calls plus supervised
//! local service processes (managed transport).
//!
//! A sidecar is a service binary the runtime starts, probes for readiness
//! and supervises: unexpected exits restart with backoff up to a bound,
//! runtime shutdown stops the process. The wire contract stays plain HTTP;
//! supervision only owns the process lifecycle, never the protocol.

use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod sidecar;

pub use sidecar::{RunningSidecar, SidecarSpec, start_sidecar};

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
        }
    }
}

impl ServiceTransport {
    /// Whether the service may be contacted: a reachable transport (an
    /// address for external mode, a binary for managed mode).
    pub fn is_usable(&self) -> bool {
        match self.transport_mode {
            TransportMode::External => self
                .base_url
                .as_deref()
                .is_some_and(|url| !url.trim().is_empty()),
            TransportMode::Managed => !self.managed_binary.trim().is_empty(),
        }
    }

    /// Direct-call base URL. Only meaningful for external transport;
    /// managed transport resolves to an equivalent external transport once
    /// the supervised process reports its address.
    pub fn external_base_url(&self) -> Option<String> {
        if self.transport_mode != TransportMode::External {
            return None;
        }
        self.base_url.as_deref().and_then(|url| {
            let trimmed = url.trim();
            (!trimmed.is_empty()).then(|| trimmed.trim_end_matches('/').to_string())
        })
    }

    /// Resolve managed transport into an equivalent external transport once
    /// the supervised process reports its address. Downstream consumers
    /// only ever observe external transports.
    pub fn with_managed_address(&self, base_url: String) -> Self {
        Self {
            transport_mode: TransportMode::External,
            base_url: Some(base_url),
            ..self.clone()
        }
    }
}

/// Build a JSON-capable HTTP client with a client-level timeout. Failures
/// here mean the integration itself is misconfigured, never the service.
pub fn http_client(timeout_ms: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms.max(1)))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

/// POST a JSON body and decode the JSON answer with a single overall
/// timeout. Any failure (unreachable service, timeout, request-level
/// rejection, undecodable answer) surfaces as a string the caller maps to
/// its own degradation path.
pub async fn post_json(
    client: &reqwest::Client,
    url: &str,
    timeout_ms: u64,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let send = client.post(url).json(body).send();
    let response = tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), send)
        .await
        .map_err(|_| format!("request to {url} timed out after {timeout_ms} ms"))?
        .map_err(|e| format!("request to {url} failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let reason: String = body.chars().take(300).collect();
        return Err(format!(
            "request to {url} rejected with status {status}: {reason}"
        ));
    }
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("response from {url} was not decodable: {e}"))
}

/// How to reach a service over HTTP for readiness probing.
pub fn probe_http(url: &str, timeout_ms: u64) -> impl std::future::Future<Output = bool> {
    let url = url.to_string();
    async move {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms.max(1)))
            .build()
        {
            Ok(client) => client,
            Err(_) => return false,
        };
        // Any completed HTTP response proves the process serves; status
        // codes stay the caller's concern (degraded content still counts
        // as alive for lifecycle purposes).
        client.get(&url).send().await.is_ok()
    }
}

/// Pick a free loopback port by binding an ephemeral listener. Best-effort:
/// the port is released before the child binds it, so this suits loopback
/// single-host use only.
pub fn pick_loopback_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

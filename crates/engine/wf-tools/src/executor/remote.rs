//! Remote service executors.
//!
//! Defines the [`RemoteExecutor`] trait plus connection/status/result types
//! for generic remote tool backends.

use serde_json::Value;

use crate::error::ToolResult;

/// Connection configuration for a remote service.
#[derive(Debug, Clone)]
pub struct RemoteConnectionConfig {
    /// Service address (`host:port` or a full `http://host:port` URL).
    pub address: String,
    /// Whether to use TLS (affects the gRPC channel scheme).
    pub use_tls: bool,
    /// Per-call timeout in milliseconds.
    pub timeout: Option<u64>,
    /// Reconnection policy.
    pub reconnect_policy: Option<ReconnectPolicy>,
}

impl RemoteConnectionConfig {
    /// Effective channel address (with scheme prefix for gRPC).
    pub fn channel_address(&self) -> String {
        if self.address.starts_with("http://") || self.address.starts_with("https://") {
            self.address.clone()
        } else if self.use_tls {
            format!("https://{}", self.address)
        } else {
            format!("http://{}", self.address)
        }
    }
}

/// Reconnection policy with exponential backoff.
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 1_000,
            max_delay_ms: 30_000,
        }
    }
}

/// Connection state of a remote executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemoteExecutorStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Unhealthy,
    Error,
}

impl RemoteExecutorStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteExecutorStatus::Disconnected => "disconnected",
            RemoteExecutorStatus::Connecting => "connecting",
            RemoteExecutorStatus::Connected => "connected",
            RemoteExecutorStatus::Unhealthy => "unhealthy",
            RemoteExecutorStatus::Error => "error",
        }
    }
}

/// Result of a remote call with latency / retry metrics.
#[derive(Debug, Clone)]
pub struct RemoteExecutionResult {
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<RemoteErrorInfo>,
    pub call_duration_ms: u64,
    pub retry_count: u32,
}

/// Structured remote error information.
#[derive(Debug, Clone)]
pub struct RemoteErrorInfo {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

/// A stateful connection to a remote service. Implementations are expected
/// to use interior mutability so methods take `&self` and the executor can be
/// shared (e.g. via `Arc`) across concurrent tool executions.
#[allow(async_fn_in_trait)]
pub trait RemoteExecutor: Send + Sync {
    async fn connect(&self, config: &RemoteConnectionConfig) -> ToolResult<()>;
    async fn disconnect(&self) -> ToolResult<()>;
    async fn call(&self, method: &str, request: &Value) -> ToolResult<Value>;
    fn is_connected(&self) -> bool;
    fn get_status(&self) -> RemoteExecutorStatus;
}

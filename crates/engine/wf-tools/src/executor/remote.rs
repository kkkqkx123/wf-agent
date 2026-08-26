//! Remote service executors.
//!
//! Defines the [`RemoteExecutor`] trait plus connection/status/result types,
//! and — behind the `remote-layertwine` feature — a [`LayertwineExecutor`]
//! implementing it over the Layertwine gRPC service (embedded or remote
//! deployment).

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

#[cfg(feature = "remote-layertwine")]
pub mod layertwine_impl {
    //! Layertwine gRPC executor (requires the `remote-layertwine` feature).

    use super::*;
    use crate::error::ToolError;
    use crate::executor::trait_def::ToolExecutionContext;
    use crate::registry::ToolRegistry;
    use layertwine::api::rpc::client::{ClientError, LayertwineGrpcClient};
    use layertwine::api::rpc::layertwine_proto::{
        AgentSubmitRequest, ApproveRequest, BackupRequest, BranchCreateRequest,
        BranchSwitchRequest, CheckpointDiffRequest, CheckpointRestoreRequest, CommitRequest,
        EditRequest, InitRequest, LogRequest, StatusResponse,
    };
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Deployment mode for the Layertwine service.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum LayertwineDeployMode {
        /// Auto-spawn a `layertwine` binary and manage its lifecycle.
        Embedded,
        /// Connect to a pre-deployed gRPC server.
        #[default]
        Remote,
    }

    /// Configuration for a LayertwineExecutor.
    #[derive(Debug, Clone)]
    pub struct LayertwineExecutorConfig {
        pub deploy_mode: LayertwineDeployMode,
        /// Address for remote mode / target address for embedded mode.
        pub address: Option<String>,
        /// Embedded mode: path to the `layertwine` binary.
        pub binary_path: Option<String>,
        /// Embedded mode: database path passed to the process.
        pub db_path: Option<String>,
        pub timeout: Option<u64>,
        pub reconnect_policy: Option<ReconnectPolicy>,
    }

    impl Default for LayertwineExecutorConfig {
        fn default() -> Self {
            Self {
                deploy_mode: LayertwineDeployMode::Remote,
                address: None,
                binary_path: None,
                db_path: None,
                timeout: None,
                reconnect_policy: None,
            }
        }
    }

    #[derive(Default)]
    struct LayertwineInner {
        client: Option<LayertwineGrpcClient>,
        process: Option<LayertwineProcessManager>,
        last_config: Option<RemoteConnectionConfig>,
        reconnect_attempts: u32,
    }

    const STATUS_DISCONNECTED: u8 = 0;
    const STATUS_CONNECTING: u8 = 1;
    const STATUS_CONNECTED: u8 = 2;
    const STATUS_UNHEALTHY: u8 = 3;
    const STATUS_ERROR: u8 = 4;

    /// Default per-call gRPC timeout when the config does not set one.
    const DEFAULT_CALL_TIMEOUT_MS: u64 = 30_000;

    fn status_from_u8(v: u8) -> RemoteExecutorStatus {
        match v {
            STATUS_CONNECTING => RemoteExecutorStatus::Connecting,
            STATUS_CONNECTED => RemoteExecutorStatus::Connected,
            STATUS_UNHEALTHY => RemoteExecutorStatus::Unhealthy,
            STATUS_ERROR => RemoteExecutorStatus::Error,
            _ => RemoteExecutorStatus::Disconnected,
        }
    }

    /// Layertwine gRPC executor with automatic reconnection.
    pub struct LayertwineExecutor {
        config: LayertwineExecutorConfig,
        inner: tokio::sync::Mutex<LayertwineInner>,
        status: std::sync::atomic::AtomicU8,
    }

    impl LayertwineExecutor {
        pub fn new(config: LayertwineExecutorConfig) -> Self {
            Self {
                config,
                inner: tokio::sync::Mutex::new(LayertwineInner::default()),
                status: std::sync::atomic::AtomicU8::new(STATUS_DISCONNECTED),
            }
        }

        pub fn remote(address: impl Into<String>) -> Self {
            Self::new(LayertwineExecutorConfig {
                deploy_mode: LayertwineDeployMode::Remote,
                address: Some(address.into()),
                ..Default::default()
            })
        }

        /// Current executor connection status.
        pub fn connection_status(&self) -> RemoteExecutorStatus {
            status_from_u8(self.status.load(Ordering::SeqCst))
        }

        fn set_status(&self, status: RemoteExecutorStatus) {
            let v = match status {
                RemoteExecutorStatus::Disconnected => STATUS_DISCONNECTED,
                RemoteExecutorStatus::Connecting => STATUS_CONNECTING,
                RemoteExecutorStatus::Connected => STATUS_CONNECTED,
                RemoteExecutorStatus::Unhealthy => STATUS_UNHEALTHY,
                RemoteExecutorStatus::Error => STATUS_ERROR,
            };
            self.status.store(v, Ordering::SeqCst);
        }

        fn config(&self) -> &LayertwineExecutorConfig {
            &self.config
        }

        async fn ensure_connected(&self) -> ToolResult<()> {
            let policy = self.config.reconnect_policy.clone().unwrap_or_default();
            let mut attempts = 0;
            loop {
                if self.inner.lock().await.client.is_some() {
                    return Ok(());
                }

                let last = self.inner.lock().await.last_config.clone().ok_or_else(|| {
                    ToolError::ExecutionError(
                        "No previous connection configuration; call connect() first".into(),
                    )
                })?;

                match self.connect(&last).await {
                    Ok(()) => {
                        self.inner.lock().await.reconnect_attempts = 0;
                        return Ok(());
                    }
                    Err(e) => {
                        attempts += 1;
                        if attempts >= policy.max_retries {
                            return Err(e);
                        }
                        let delay = (policy.base_delay_ms * 2u64.pow(attempts - 1))
                            .min(policy.max_delay_ms);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }

        // ── Typed convenience methods ──

        pub async fn init(
            &self,
            db_path: Option<&str>,
            git_repo: Option<&str>,
            git_ref: Option<&str>,
        ) -> ToolResult<Value> {
            self.call(
                "init",
                &serde_json::json!({
                    "db_path": db_path,
                    "git_repo": git_repo,
                    "git_ref": git_ref,
                }),
            )
            .await
        }

        pub async fn edit(&self, file: &str, content: Option<&str>) -> ToolResult<Value> {
            self.call(
                "edit",
                &serde_json::json!({ "file": file, "content": content }),
            )
            .await
        }

        pub async fn repo_status(&self) -> ToolResult<Value> {
            self.call("status", &Value::Null).await
        }

        pub async fn commit(&self, message: &str, author: Option<&str>) -> ToolResult<Value> {
            self.call(
                "commit",
                &serde_json::json!({ "message": message, "author": author }),
            )
            .await
        }

        pub async fn log(&self, count: Option<u32>) -> ToolResult<Value> {
            self.call("log", &serde_json::json!({ "count": count }))
                .await
        }

        pub async fn branch_create(&self, name: &str) -> ToolResult<Value> {
            self.call("branch_create", &serde_json::json!({ "name": name }))
                .await
        }

        pub async fn branch_switch(&self, name: &str) -> ToolResult<Value> {
            self.call("branch_switch", &serde_json::json!({ "name": name }))
                .await
        }

        pub async fn branch_list(&self) -> ToolResult<Value> {
            self.call("branch_list", &Value::Null).await
        }

        pub async fn approve(&self, agent_id: &str) -> ToolResult<Value> {
            self.call("approve", &serde_json::json!({ "agent_id": agent_id }))
                .await
        }

        pub async fn backup(&self, snapshot_id: &str, label: Option<&str>) -> ToolResult<Value> {
            self.call(
                "backup",
                &serde_json::json!({ "snapshot_id": snapshot_id, "label": label }),
            )
            .await
        }

        pub async fn checkpoint_restore(
            &self,
            checkpoint_id: &str,
            source_filter: Vec<String>,
        ) -> ToolResult<Value> {
            self.call(
                "checkpoint_restore",
                &serde_json::json!({ "checkpoint_id": checkpoint_id, "source_filter": source_filter }),
            )
            .await
        }

        pub async fn checkpoint_diff(&self, from_id: &str, to_id: &str) -> ToolResult<Value> {
            self.call(
                "checkpoint_diff",
                &serde_json::json!({ "from_id": from_id, "to_id": to_id }),
            )
            .await
        }
    }

    impl RemoteExecutor for LayertwineExecutor {
        async fn connect(&self, config: &RemoteConnectionConfig) -> ToolResult<()> {
            {
                let mut inner = self.inner.lock().await;
                if inner.client.is_some() {
                    return Ok(());
                }
                inner.last_config = Some(config.clone());
            }
            self.set_status(RemoteExecutorStatus::Connecting);

            // Embedded mode: ensure the process is running and its port open.
            if self.config().deploy_mode == LayertwineDeployMode::Embedded {
                let needs_start = self.inner.lock().await.process.is_none();
                if needs_start {
                    let binary = self.config().binary_path.clone().ok_or_else(|| {
                        ToolError::ValidationFailed("binary_path required for embedded mode".into())
                    })?;
                    let db = self.config().db_path.clone().ok_or_else(|| {
                        ToolError::ValidationFailed("db_path required for embedded mode".into())
                    })?;
                    let mut manager =
                        LayertwineProcessManager::new(binary, db, config.address.clone());
                    manager.start().await?;
                    self.inner.lock().await.process = Some(manager);
                }
            }

            let address = config.channel_address();
            match LayertwineGrpcClient::connect(&address).await {
                Ok(client) => {
                    let mut inner = self.inner.lock().await;
                    inner.client = Some(client);
                    inner.reconnect_attempts = 0;
                    drop(inner);
                    self.set_status(RemoteExecutorStatus::Connected);
                    Ok(())
                }
                Err(e) => {
                    // Clean up a half-started embedded process.
                    let stale_process = self.inner.lock().await.process.take();
                    if let Some(mut process) = stale_process {
                        let _ = process.stop().await;
                    }
                    self.set_status(RemoteExecutorStatus::Error);
                    Err(ToolError::ExecutionError(format!(
                        "Layertwine connection failed: {}",
                        e
                    )))
                }
            }
        }

        async fn disconnect(&self) -> ToolResult<()> {
            {
                let mut inner = self.inner.lock().await;
                inner.client = None;
            }
            let process = self.inner.lock().await.process.take();
            if let Some(mut process) = process {
                let _ = process.stop().await;
            }
            self.set_status(RemoteExecutorStatus::Disconnected);
            Ok(())
        }

        async fn call(&self, method: &str, request: &Value) -> ToolResult<Value> {
            self.ensure_connected().await?;

            let timeout_ms = self.config.timeout.unwrap_or(DEFAULT_CALL_TIMEOUT_MS);
            let result = {
                let mut inner = self.inner.lock().await;
                let client = inner
                    .client
                    .as_mut()
                    .ok_or_else(|| ToolError::ExecutionError("Layertwine not connected".into()))?;
                tokio::time::timeout(
                    Duration::from_millis(timeout_ms),
                    dispatch(client, method, request),
                )
                .await
            };

            match result {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(e)) => {
                    let msg = e.to_string().to_lowercase();
                    if msg.contains("transport")
                        || msg.contains("unavailable")
                        || msg.contains("refused")
                    {
                        self.inner.lock().await.client = None;
                        self.set_status(RemoteExecutorStatus::Unhealthy);
                    }
                    Err(ToolError::ExecutionError(format!(
                        "Layertwine {}: {}",
                        method, e
                    )))
                }
                Err(_) => {
                    self.inner.lock().await.client = None;
                    self.set_status(RemoteExecutorStatus::Unhealthy);
                    Err(ToolError::Timeout {
                        tool_id: method.to_string(),
                        timeout_ms,
                    })
                }
            }
        }

        fn is_connected(&self) -> bool {
            self.connection_status() == RemoteExecutorStatus::Connected
        }

        fn get_status(&self) -> RemoteExecutorStatus {
            self.connection_status()
        }
    }

    async fn dispatch(
        client: &mut LayertwineGrpcClient,
        method: &str,
        request: &Value,
    ) -> Result<Value, ClientError> {
        let value = match method {
            "init" => {
                let resp = client
                    .init(InitRequest {
                        db_path: opt_str(request, "db_path"),
                        git_repo: opt_str(request, "git_repo"),
                        git_ref: opt_str(request, "git_ref"),
                    })
                    .await?;
                serde_json::json!({
                    "db_path": resp.db_path,
                    "manual_partition_id": resp.manual_partition_id,
                    "staged_partition_id": resp.staged_partition_id,
                    "branch": resp.branch,
                })
            }
            "edit" => {
                let resp = client
                    .edit(EditRequest {
                        file: req_str(request, "file"),
                        content: opt_str(request, "content"),
                    })
                    .await?;
                serde_json::json!({
                    "snapshot_id": resp.snapshot_id,
                    "staged_snapshot_id": resp.staged_snapshot_id,
                })
            }
            "status" => status_to_value(client.status().await?),
            "commit" => {
                let resp = client
                    .commit(CommitRequest {
                        message: req_str(request, "message"),
                        author: opt_str(request, "author"),
                    })
                    .await?;
                serde_json::json!({
                    "checkpoint_id": resp.checkpoint_id,
                    "message": resp.message,
                })
            }
            "log" => {
                let resp = client
                    .log(LogRequest {
                        count: opt_u32(request, "count"),
                    })
                    .await?;
                let checkpoints: Vec<Value> = resp
                    .checkpoints
                    .into_iter()
                    .map(|cp| {
                        serde_json::json!({
                            "id": cp.id,
                            "author": cp.author,
                            "message": cp.message,
                            "parents": cp.parents,
                            "snapshots": cp.snapshots,
                            "created_at": cp.created_at,
                            "git_anchor": cp.git_anchor,
                        })
                    })
                    .collect();
                serde_json::json!({ "checkpoints": checkpoints, "total": resp.total })
            }
            "branch_create" => {
                let resp = client
                    .branch_create(BranchCreateRequest {
                        name: req_str(request, "name"),
                    })
                    .await?;
                serde_json::json!({ "name": resp.name, "head": resp.head })
            }
            "branch_switch" => {
                let resp = client
                    .branch_switch(BranchSwitchRequest {
                        name: req_str(request, "name"),
                    })
                    .await?;
                serde_json::json!({ "name": resp.name, "checkpoint_id": resp.checkpoint_id })
            }
            "branch_list" => {
                let resp = client.branch_list().await?;
                let branches: Vec<Value> = resp
                    .branches
                    .into_iter()
                    .map(|b| {
                        serde_json::json!({
                            "name": b.name,
                            "head": b.head,
                            "updated_at": b.updated_at,
                            "is_current": b.is_current,
                        })
                    })
                    .collect();
                serde_json::json!({ "branches": branches, "current": resp.current })
            }
            "agent_submit" => {
                let resp = client
                    .agent_submit(AgentSubmitRequest {
                        agent_id: req_str(request, "agent_id"),
                    })
                    .await?;
                serde_json::json!({ "snapshot_id": resp.snapshot_id })
            }
            "approve" => {
                let resp = client
                    .approve(ApproveRequest {
                        agent_id: req_str(request, "agent_id"),
                    })
                    .await?;
                serde_json::json!({
                    "integrated_snapshot_id": resp.integrated_snapshot_id,
                    "staged_snapshot_id": resp.staged_snapshot_id,
                })
            }
            "backup" => {
                let resp = client
                    .backup(BackupRequest {
                        snapshot_id: req_str(request, "snapshot_id"),
                        label: opt_str(request, "label"),
                    })
                    .await?;
                serde_json::json!({
                    "backup_id": resp.backup_id,
                    "source_snapshot_id": resp.source_snapshot_id,
                    "label": resp.label,
                })
            }
            "checkpoint_restore" => {
                let resp = client
                    .checkpoint_restore(CheckpointRestoreRequest {
                        checkpoint_id: req_str(request, "checkpoint_id"),
                        source_filter: str_list(request, "source_filter"),
                    })
                    .await?;
                checkpoint_restore_to_value(resp)
            }
            "checkpoint_diff" => {
                let resp = client
                    .checkpoint_diff(CheckpointDiffRequest {
                        from_id: req_str(request, "from_id"),
                        to_id: req_str(request, "to_id"),
                    })
                    .await?;
                serde_json::json!({
                    "from_id": resp.from_id,
                    "to_id": resp.to_id,
                    "added": resp.added,
                    "removed": resp.removed,
                    "modified": resp.modified,
                    "total_changes": resp.total_changes,
                })
            }
            other => {
                return Err(ClientError::Grpc {
                    code: tonic::Code::InvalidArgument,
                    message: format!("Unknown Layertwine RPC: {}", other),
                })
            }
        };
        Ok(value)
    }

    fn status_to_value(resp: StatusResponse) -> Value {
        let partitions: Vec<Value> = resp
            .partitions
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "layer": p.layer,
                    "name": p.name,
                    "current_snapshot": p.current_snapshot,
                    "history_len": p.history_len,
                })
            })
            .collect();
        serde_json::json!({ "partitions": partitions })
    }

    fn checkpoint_restore_to_value(
        resp: layertwine::api::rpc::layertwine_proto::CheckpointRestoreResponse,
    ) -> Value {
        let snapshots: Vec<Value> = resp
            .snapshots
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "snapshot_id": s.snapshot_id,
                    "source": s.source,
                    "content_hex": s.content_hex,
                    "content_type": s.content_type,
                })
            })
            .collect();
        serde_json::json!({
            "checkpoint": resp.checkpoint.map(|cp| {
                serde_json::json!({
                    "id": cp.id,
                    "author": cp.author,
                    "message": cp.message,
                    "parents": cp.parents,
                    "snapshots": cp.snapshots,
                    "created_at": cp.created_at,
                    "git_anchor": cp.git_anchor,
                })
            }),
            "snapshots": snapshots,
            "ancestry": resp.ancestry,
        })
    }

    fn opt_str(v: &Value, key: &str) -> Option<String> {
        v.get(key).and_then(|x| x.as_str()).map(String::from)
    }

    fn req_str(v: &Value, key: &str) -> String {
        opt_str(v, key).unwrap_or_default()
    }

    fn opt_u32(v: &Value, key: &str) -> Option<u32> {
        v.get(key).and_then(|x| x.as_u64()).map(|n| n as u32)
    }

    fn str_list(v: &Value, key: &str) -> Vec<String> {
        v.get(key)
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Manage the lifecycle of an embedded Layertwine process (gRPC mode).
    struct LayertwineProcessManager {
        binary_path: String,
        db_path: String,
        grpc_addr: String,
        child: Mutex<Option<tokio::process::Child>>,
        running: AtomicBool,
        restart_count: u32,
        max_restarts: u32,
        restart_delay: Duration,
        connect_timeout: Duration,
        port_poll_interval: Duration,
    }

    impl LayertwineProcessManager {
        fn new(binary_path: String, db_path: String, grpc_addr: String) -> Self {
            Self {
                binary_path,
                db_path,
                grpc_addr,
                child: Mutex::new(None),
                running: AtomicBool::new(false),
                restart_count: 0,
                max_restarts: 3,
                restart_delay: Duration::from_secs(1),
                connect_timeout: Duration::from_secs(10),
                port_poll_interval: Duration::from_millis(200),
            }
        }

        fn parse_addr(&self) -> std::net::SocketAddr {
            let host_port = self
                .grpc_addr
                .trim_start_matches("http://")
                .trim_start_matches("https://");
            let (host, port) = match host_port.split_once(':') {
                Some((h, p)) => (h, p.parse::<u16>().unwrap_or(50051)),
                None => (host_port, 50051),
            };
            let host = if host.is_empty() { "127.0.0.1" } else { host };
            format!("{}:{}", host, port)
                .parse()
                .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 50051)))
        }

        /// Non-blocking probe of the gRPC port: a TCP connect via async IO so
        /// a slow/unreachable port never pins a tokio worker.
        async fn port_ready(&self) -> bool {
            let addr = self.parse_addr();
            tokio::time::timeout(
                Duration::from_millis(500),
                tokio::net::TcpStream::connect(addr),
            )
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false)
        }

        async fn wait_for_port(&self) -> ToolResult<()> {
            let deadline = Instant::now() + self.connect_timeout;
            while Instant::now() < deadline {
                if self.port_ready().await {
                    return Ok(());
                }
                tokio::time::sleep(self.port_poll_interval).await;
            }
            Err(ToolError::ExecutionError(format!(
                "Timed out waiting for Layertwine gRPC server on {}",
                self.grpc_addr
            )))
        }

        async fn spawn_once(&mut self) -> ToolResult<()> {
            let mut command = tokio::process::Command::new(&self.binary_path);
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .env("LAYERTWINE_MODE", "grpc")
                .env("LAYERTWINE_GRPC_ADDR", &self.grpc_addr)
                .env("LAYERTWINE_DB_PATH", &self.db_path);
            let child = command.spawn().map_err(|e| {
                ToolError::ExecutionError(format!(
                    "Failed to spawn layertwine binary '{}': {}",
                    self.binary_path, e
                ))
            })?;
            *wf_common::lock::lock_ok(self.child.lock()) = Some(child);
            self.wait_for_port().await
        }

        async fn start(&mut self) -> ToolResult<()> {
            if self.running.load(Ordering::SeqCst) {
                return Ok(());
            }
            self.running.store(true, Ordering::SeqCst);

            loop {
                match self.spawn_once().await {
                    Ok(()) => return Ok(()),
                    Err(_) => {
                        let exited = self
                            .child
                            .lock()
                            .unwrap()
                            .as_mut()
                            .map(|c| c.try_wait().ok().flatten().is_some())
                            .unwrap_or(true);
                        if exited && self.restart_count < self.max_restarts {
                            self.restart_count += 1;
                            tokio::time::sleep(self.restart_delay).await;
                            continue;
                        }
                        self.running.store(false, Ordering::SeqCst);
                        return Err(ToolError::ExecutionError(format!(
                            "Layertwine process did not become ready on {}",
                            self.grpc_addr
                        )));
                    }
                }
            }
        }

        async fn stop(&mut self) -> ToolResult<()> {
            self.running.store(false, Ordering::SeqCst);
            let child = wf_common::lock::lock_ok(self.child.lock()).take();
            if let Some(mut child) = child {
                let _ = child.kill().await;
            }
            Ok(())
        }
    }

    /// Register Layertwine operations as stateless tools (one tool per
    /// operation) dispatching to the shared executor.
    pub fn register_layertwine_tools(
        registry: &ToolRegistry,
        executor: Arc<LayertwineExecutor>,
    ) -> crate::error::ToolResult<()> {
        let operations: &[(&str, &str)] = &[
            (
                "layertwine_init",
                "Initialize a Layertwine repository. Parameters: db_path, git_repo, git_ref.",
            ),
            (
                "layertwine_edit",
                "Record a file edit. Parameters: file, content.",
            ),
            ("layertwine_status", "Show repository partition status."),
            (
                "layertwine_commit",
                "Create a checkpoint. Parameters: message, author.",
            ),
            (
                "layertwine_log",
                "Query checkpoint history. Parameters: count.",
            ),
            ("layertwine_branch_list", "List branches."),
            (
                "layertwine_branch_create",
                "Create a branch. Parameters: name.",
            ),
            (
                "layertwine_branch_switch",
                "Switch to a branch. Parameters: name.",
            ),
            (
                "layertwine_approve",
                "Approve an agent's changes. Parameters: agent_id.",
            ),
            (
                "layertwine_backup",
                "Backup a snapshot. Parameters: snapshot_id, label.",
            ),
            (
                "layertwine_checkpoint_restore",
                "Restore a checkpoint. Parameters: checkpoint_id, source_filter.",
            ),
            (
                "layertwine_checkpoint_diff",
                "Diff two checkpoints. Parameters: from_id, to_id.",
            ),
        ];

        for (tool_id, description) in operations {
            let executor = executor.clone();
            let tool_id = tool_id.to_string();
            let tool = wf_types::tool::Tool {
                id: tool_id.clone(),
                name: tool_id.clone(),
                description: description.to_string(),
                tool_type: wf_types::tool::ToolType::Stateless,
                parameters: None,
                metadata: Some(wf_types::tool::ToolMetadata {
                    category: Some("integration".into()),
                    tags: Some(vec!["layertwine".into(), "remote".into()]),
                    documentation_url: None,
                    custom_fields: None,
                    risk_level: Some(wf_types::tool::ToolRiskLevel::Write),
                    auto_approvable: None,
                    create_checkpoint: None,
                    exposure: None,
                }),
                config: Some(serde_json::json!({ "executor": "layertwine" })),
                enabled: Some(true),
                strict: None,
                default_timeout_ms: None,
            };
            registry.register_tool(tool);

            let method = tool_id.trim_start_matches("layertwine_").to_string();
            let handler: crate::executor::stateless::StatelessAsyncHandler =
                Arc::new(move |params: Value, _ctx: ToolExecutionContext| {
                    let executor = executor.clone();
                    let method = method.clone();
                    Box::pin(async move { executor.call(&method, &params).await })
                });
            registry.register_stateless_async_handler(&tool_id, handler);
        }
        Ok(())
    }
}

#[cfg(feature = "remote-layertwine")]
pub use layertwine_impl::{
    register_layertwine_tools, LayertwineDeployMode, LayertwineExecutor, LayertwineExecutorConfig,
};

#[cfg(all(test, feature = "remote-layertwine"))]
mod tests {
    use super::*;
    use layertwine::api::service::{ApiService, ServiceConfig};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::Arc;

    fn free_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    /// Spawn an in-process Layertwine gRPC server; returns (address, db_path, tempdir).
    /// The caller must keep the tempdir alive for the duration of the test.
    async fn spawn_server() -> (String, String, tempfile::TempDir) {
        let addr: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("layertwine-test.db");
        let db_path_str = db_path.to_string_lossy().to_string();
        let service = Arc::new(
            ApiService::open(ServiceConfig {
                db_path: db_path_str.clone(),
            })
            .expect("create ApiService"),
        );
        tokio::spawn(async move {
            let _ = layertwine::api::rpc::serve(service, addr).await;
        });
        // Give the server a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        (format!("127.0.0.1:{}", addr.port()), db_path_str, dir)
    }

    fn conn_config(address: &str) -> RemoteConnectionConfig {
        RemoteConnectionConfig {
            address: address.to_string(),
            use_tls: false,
            timeout: None,
            reconnect_policy: Some(ReconnectPolicy {
                max_retries: 2,
                base_delay_ms: 50,
                max_delay_ms: 200,
            }),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_remote_init_edit_commit_roundtrip() {
        let (addr, db_path, _dir) = spawn_server().await;
        let executor = LayertwineExecutor::remote(addr.clone());
        executor.connect(&conn_config(&addr)).await.unwrap();
        assert!(executor.is_connected(), "should be connected");
        assert_eq!(executor.get_status(), RemoteExecutorStatus::Connected);

        // init (same db path as the server so partitions are shared)
        let init = executor.init(Some(&db_path), None, None).await.unwrap();
        assert_eq!(init["branch"], "main");
        assert!(!init["manual_partition_id"].as_str().unwrap().is_empty());

        // edit
        let edit = executor
            .edit("src/main.rs", Some("fn main() {}\n"))
            .await
            .unwrap();
        assert!(!edit["snapshot_id"].as_str().unwrap().is_empty());

        // status
        let status = executor.repo_status().await.unwrap();
        let partitions = status["partitions"].as_array().unwrap();
        assert!(!partitions.is_empty());

        // commit
        let commit = executor
            .commit("initial commit", Some("dev-1"))
            .await
            .unwrap();
        assert!(!commit["checkpoint_id"].as_str().unwrap().is_empty());

        // log
        let log = executor.log(Some(10)).await.unwrap();
        assert!(log["total"].as_u64().unwrap() >= 1);

        executor.disconnect().await.unwrap();
        assert!(!executor.is_connected());
        assert_eq!(executor.get_status(), RemoteExecutorStatus::Disconnected);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_remote_branch_and_checkpoint_ops() {
        let (addr, db_path, _dir) = spawn_server().await;
        let executor = LayertwineExecutor::remote(addr.clone());
        executor.connect(&conn_config(&addr)).await.unwrap();

        executor.init(Some(&db_path), None, None).await.unwrap();
        executor.edit("a.txt", Some("hello\n")).await.unwrap();
        let commit = executor.commit("base", None).await.unwrap();
        let base_cp = commit["checkpoint_id"].as_str().unwrap().to_string();

        let branch = executor.branch_create("feature/x").await.unwrap();
        assert_eq!(branch["name"], "feature/x");
        executor.branch_switch("feature/x").await.unwrap();
        executor.edit("a.txt", Some("hello world\n")).await.unwrap();
        let commit2 = executor.commit("feature change", None).await.unwrap();
        let feature_cp = commit2["checkpoint_id"].as_str().unwrap().to_string();

        let diff = executor
            .checkpoint_diff(&base_cp, &feature_cp)
            .await
            .unwrap();
        assert!(diff["total_changes"].as_u64().unwrap() >= 1);
        assert!(
            diff["added"].is_array() && diff["removed"].is_array() && diff["modified"].is_array()
        );

        let branches = executor.branch_list().await.unwrap();
        assert!(branches["branches"].as_array().unwrap().len() >= 2);
    }
}

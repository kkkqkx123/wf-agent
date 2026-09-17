//! rmcp-backed MCP client adapter.
//!
//! This module replaces the previously hand-rolled JSON-RPC client and the
//! stdio / SSE / streamable-HTTP transports with the official `rmcp` crate.
//! The framework-facing surface (`RmcpClient`) keeps the same method shapes
//! the connection manager relied on, so `McpConnectionManager` only swaps its
//! inner client type.
//!
//! Responsibilities of this module:
//! - build an `rmcp` transport from an `McpServerConfig`;
//! - drive the handshake with `ClientLifecycleMode::Auto` (preferring the
//!   latest protocol version while falling back to the legacy `2024-11-05`
//!   handshake for older servers);
//! - expose capability calls (`list_*`, `call_tool`, `read_resource`) that map
//!   rmcp model types onto the framework's internal types;
//! - forward server notifications to an in-process channel so the connection
//!   manager can invalidate metadata and re-register tools.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rmcp::model::{
    CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation, JsonObject,
    LoggingMessageNotificationParam, ProgressNotificationParam, ProtocolVersion,
    ReadResourceRequestParams, Resource, ResourceContents, ResourceTemplate,
    ResourceUpdatedNotificationParam, Tool,
};
use rmcp::service::{
    ClientLifecycleMode, NotificationContext, RoleClient, RunningService, ServiceError,
};
use rmcp::transport::{
    streamable_http_client::StreamableHttpClientTransportConfig, ConfigureCommandExt,
    StreamableHttpClientTransport, TokioChildProcess,
};
use rmcp::{serve_client_with_lifecycle, ClientHandler};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::mpsc;

use wf_types::tool::mcp_connection::{McpServerConfig, McpStdioConfig, McpStreamableHttpConfig};
use wf_types::Metadata;

use crate::error::{ToolError, ToolResult};

/// Alias for the concrete rmcp service type managed by [`RmcpClient`].
type RmcpService = RunningService<RoleClient, WfClientHandler>;

/// Framework-internal representation of a discovered tool, preserved from the
/// previous client implementation so registration/metadata keep working.
#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
}

/// Server-initiated notifications surfaced to the connection manager.
///
/// The handler forwards every relevant rmcp callback into this enum; the
/// connection manager drains the channel and reacts (e.g. re-discover
/// capabilities on a list-changed event).
#[derive(Debug, Clone, Copy)]
pub enum McpsNotification {
    ToolListChanged,
    ResourceListChanged,
    PromptListChanged,
    ResourceUpdated,
    LoggingMessage,
    Progress,
}

/// Client handler injected into the rmcp service.
///
/// It advertises the framework identity during `initialize` and forwards
/// server notifications onto an mpsc channel owned by the surrounding
/// [`RmcpClient`].
#[derive(Clone)]
pub struct WfClientHandler {
    server_name: String,
    notify_tx: mpsc::Sender<McpsNotification>,
}

impl WfClientHandler {
    fn new(server_name: impl Into<String>, notify_tx: mpsc::Sender<McpsNotification>) -> Self {
        Self {
            server_name: server_name.into(),
            notify_tx,
        }
    }
}

impl ClientHandler for WfClientHandler {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("wf-tools", env!("CARGO_PKG_VERSION")),
        )
    }

    async fn on_tool_list_changed(&self, _ctx: NotificationContext<RoleClient>) {
        let _ = self.notify_tx.send(McpsNotification::ToolListChanged).await;
    }

    async fn on_resource_list_changed(&self, _ctx: NotificationContext<RoleClient>) {
        let _ = self
            .notify_tx
            .send(McpsNotification::ResourceListChanged)
            .await;
    }

    async fn on_prompt_list_changed(&self, _ctx: NotificationContext<RoleClient>) {
        let _ = self
            .notify_tx
            .send(McpsNotification::PromptListChanged)
            .await;
    }

    async fn on_resource_updated(
        &self,
        _params: ResourceUpdatedNotificationParam,
        _ctx: NotificationContext<RoleClient>,
    ) {
        let _ = self.notify_tx.send(McpsNotification::ResourceUpdated).await;
    }

    async fn on_progress(
        &self,
        _params: ProgressNotificationParam,
        _ctx: NotificationContext<RoleClient>,
    ) {
        let _ = self.notify_tx.send(McpsNotification::Progress).await;
    }

    async fn on_logging_message(
        &self,
        _params: LoggingMessageNotificationParam,
        _ctx: NotificationContext<RoleClient>,
    ) {
        let _ = self.notify_tx.send(McpsNotification::LoggingMessage).await;
    }
}

/// A live MCP connection backed by an rmcp `RunningService`.
pub struct RmcpClient {
    server_name: String,
    service: OnceLock<Arc<RmcpService>>,
    notify_rx: std::sync::Mutex<Option<mpsc::Receiver<McpsNotification>>>,
}

impl RmcpClient {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            service: OnceLock::new(),
            notify_rx: std::sync::Mutex::new(None),
        }
    }

    /// Build the transport, perform the handshake and store the running
    /// service. Returns an error if already connected or the handshake fails.
    pub async fn connect(&self, config: &McpServerConfig) -> ToolResult<()> {
        let (notify_tx, notify_rx) = mpsc::channel(64);
        let handler = WfClientHandler::new(self.server_name.clone(), notify_tx);

        let lifecycle = ClientLifecycleMode::Auto {
            preferred_versions: vec![ProtocolVersion::LATEST],
            legacy_version: Some(ProtocolVersion::V_2024_11_05),
        };

        let service = match config {
            McpServerConfig::Stdio(c) => {
                let command = build_stdio_command(c)?;
                let child = TokioChildProcess::new(command).map_err(|e| {
                    ToolError::TransportError(format!("failed to spawn MCP server process: {e}"))
                })?;
                serve_client_with_lifecycle(handler, child, lifecycle)
                    .await
                    .map_err(|e| map_init_error(&self.server_name, e))?
            }
            McpServerConfig::StreamableHttp(c) => {
                let client = build_http_client(c)?;
                let transport_config = StreamableHttpClientTransportConfig::with_uri(c.url.clone());
                let transport =
                    StreamableHttpClientTransport::with_client(client, transport_config);
                serve_client_with_lifecycle(handler, transport, lifecycle)
                    .await
                    .map_err(|e| map_init_error(&self.server_name, e))?
            }
        };

        self.service
            .set(Arc::new(service))
            .map_err(|_| ToolError::Internal("MCP client already connected".into()))?;
        *self.notify_rx.lock().unwrap() = Some(notify_rx);
        Ok(())
    }

    /// Trigger shutdown via the cancellation token. The background task is
    /// torn down asynchronously by the running service's drop guard.
    pub async fn disconnect(&self) -> ToolResult<()> {
        if let Some(service) = self.service.get() {
            service.cancellation_token().cancel();
        }
        // Dropping the receiver ends the notification pump task.
        *self.notify_rx.lock().unwrap() = None;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.service.get().map(|s| !s.is_closed()).unwrap_or(false)
    }

    /// Server instructions returned during `initialize` (if any).
    pub fn instructions(&self) -> Option<String> {
        self.service
            .get()
            .and_then(|s| s.peer().peer_info())
            .and_then(|info| info.instructions.clone())
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: &Value,
        timeout_ms: u64,
    ) -> ToolResult<Value> {
        let service = self.require_connected()?;
        let params = CallToolRequestParams::new(tool_name.to_string()).with_arguments(
            serde_json::from_value::<JsonObject>(arguments.clone()).unwrap_or_default(),
        );

        match tokio::time::timeout(Duration::from_millis(timeout_ms), service.call_tool(params))
            .await
        {
            Ok(Ok(result)) => Ok(serde_json::to_value(result)?),
            Ok(Err(e)) => Err(map_service_error(&self.server_name, e)),
            Err(_) => Err(ToolError::Timeout {
                tool_id: tool_name.to_string(),
                timeout_ms,
            }),
        }
    }

    pub async fn list_tools(&self, timeout_ms: u64) -> ToolResult<Vec<McpToolInfo>> {
        let service = self.require_connected()?;
        match tokio::time::timeout(Duration::from_millis(timeout_ms), service.list_all_tools())
            .await
        {
            Ok(Ok(tools)) => Ok(tools.into_iter().map(tool_to_info).collect()),
            Ok(Err(e)) => Err(map_service_error(&self.server_name, e)),
            Err(_) => Err(ToolError::Timeout {
                tool_id: self.server_name.clone(),
                timeout_ms,
            }),
        }
    }

    pub async fn list_resources(
        &self,
        timeout_ms: u64,
    ) -> ToolResult<Vec<wf_types::tool::McpResource>> {
        let service = self.require_connected()?;
        match tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            service.list_all_resources(),
        )
        .await
        {
            Ok(Ok(resources)) => Ok(resources.into_iter().map(resource_to_info).collect()),
            Ok(Err(e)) => Err(map_service_error(&self.server_name, e)),
            Err(_) => Err(ToolError::Timeout {
                tool_id: self.server_name.clone(),
                timeout_ms,
            }),
        }
    }

    pub async fn list_resource_templates(
        &self,
        timeout_ms: u64,
    ) -> ToolResult<Vec<wf_types::tool::McpResourceTemplate>> {
        let service = self.require_connected()?;
        match tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            service.list_all_resource_templates(),
        )
        .await
        {
            Ok(Ok(templates)) => Ok(templates.into_iter().map(template_to_info).collect()),
            Ok(Err(e)) => Err(map_service_error(&self.server_name, e)),
            Err(_) => Err(ToolError::Timeout {
                tool_id: self.server_name.clone(),
                timeout_ms,
            }),
        }
    }

    pub async fn read_resource(
        &self,
        uri: &str,
        timeout_ms: u64,
    ) -> ToolResult<wf_types::tool::McpResourceReadResult> {
        let service = self.require_connected()?;
        let params = ReadResourceRequestParams::new(uri.to_string());
        match tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            service.read_resource(params),
        )
        .await
        {
            Ok(Ok(result)) => Ok(read_result_to_result(result)),
            Ok(Err(e)) => Err(map_service_error(&self.server_name, e)),
            Err(_) => Err(ToolError::Timeout {
                tool_id: self.server_name.clone(),
                timeout_ms,
            }),
        }
    }

    /// Drain server notifications and invoke `on_changed` whenever a list
    /// changed, so the connection manager can re-discover capabilities and
    /// refresh registered tools.
    pub async fn run_notification_pump(&self, on_changed: Arc<dyn Fn(&str) + Send + Sync>) {
        let mut rx = self.notify_rx.lock().unwrap().take();
        let Some(rx) = &mut rx else {
            return;
        };
        while let Some(notif) = rx.recv().await {
            match notif {
                McpsNotification::ToolListChanged
                | McpsNotification::ResourceListChanged
                | McpsNotification::PromptListChanged => {
                    on_changed(&self.server_name);
                }
                _ => {}
            }
        }
    }

    fn require_connected(&self) -> ToolResult<&Arc<RmcpService>> {
        self.service.get().ok_or_else(|| {
            ToolError::McpError(format!("MCP server '{}' not connected", self.server_name))
        })
    }
}

/// Build a tokio command for a stdio server, applying args / cwd / env.
fn build_stdio_command(c: &McpStdioConfig) -> ToolResult<Command> {
    let command = Command::new(&c.command).configure(|cmd| {
        if let Some(args) = &c.args {
            cmd.args(args);
        }
        if let Some(cwd) = &c.cwd {
            cmd.current_dir(cwd);
        }
        if let Some(env) = &c.env {
            for (key, value) in metadata_to_pairs(env) {
                cmd.env(key, value);
            }
        }
    });
    Ok(command)
}

/// Build a reqwest client carrying the configured default headers.
fn build_http_client(c: &McpStreamableHttpConfig) -> ToolResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder();
    if let Some(headers) = &c.headers {
        let mut header_map = reqwest::header::HeaderMap::new();
        for (key, value) in metadata_to_pairs(headers) {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(&value),
            ) {
                header_map.insert(name, val);
            }
        }
        builder = builder.default_headers(header_map);
    }
    builder.build().map_err(ToolError::HttpError)
}

/// Flatten a metadata map into string key/value pairs (numbers and booleans
/// are stringified so they can be used as env vars or header values).
fn metadata_to_pairs(m: &Metadata) -> Vec<(String, String)> {
    m.iter()
        .filter_map(|(k, v)| {
            let value = match v {
                Value::String(s) => Some(s.clone()),
                Value::Bool(b) => Some(b.to_string()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            }?;
            Some((k.clone(), value))
        })
        .collect()
}

fn tool_to_info(tool: Tool) -> McpToolInfo {
    McpToolInfo {
        name: tool.name.to_string(),
        description: tool.description.map(|d| d.to_string()),
        input_schema: Some(Value::Object((*tool.input_schema).clone())),
    }
}

fn resource_to_info(r: Resource) -> wf_types::tool::McpResource {
    wf_types::tool::McpResource {
        uri: r.uri,
        name: r.name,
        description: r.description,
        mime_type: r.mime_type,
    }
}

fn template_to_info(t: ResourceTemplate) -> wf_types::tool::McpResourceTemplate {
    wf_types::tool::McpResourceTemplate {
        uri_template: t.uri_template,
        name: t.name,
        description: t.description,
        mime_type: t.mime_type,
    }
}

fn read_result_to_result(
    r: rmcp::model::ReadResourceResult,
) -> wf_types::tool::McpResourceReadResult {
    wf_types::tool::McpResourceReadResult {
        contents: r
            .contents
            .into_iter()
            .filter_map(|content| match content {
                ResourceContents::TextResourceContents {
                    uri,
                    mime_type,
                    text,
                    ..
                } => Some(wf_types::tool::McpResourceContent {
                    uri,
                    mime_type,
                    text: Some(text),
                    blob: None,
                }),
                ResourceContents::BlobResourceContents {
                    uri,
                    mime_type,
                    blob,
                    ..
                } => Some(wf_types::tool::McpResourceContent {
                    uri,
                    mime_type,
                    text: None,
                    blob: Some(blob),
                }),
                _ => None,
            })
            .collect(),
    }
}

fn map_service_error(server: &str, e: ServiceError) -> ToolError {
    match e {
        ServiceError::McpError(m) => ToolError::McpError(m.to_string()),
        ServiceError::TransportSend(d) => ToolError::TransportError(d.to_string()),
        ServiceError::TransportClosed => ToolError::TransportError("transport closed".into()),
        ServiceError::Timeout { timeout } => ToolError::Timeout {
            tool_id: server.to_string(),
            timeout_ms: timeout.as_millis() as u64,
        },
        ServiceError::Cancelled { reason } => ToolError::ConnectionFailed {
            server: server.to_string(),
            reason: reason.unwrap_or_default(),
        },
        _ => ToolError::McpError(e.to_string()),
    }
}

fn map_init_error(server: &str, e: rmcp::service::ClientInitializeError) -> ToolError {
    ToolError::ConnectionFailed {
        server: server.to_string(),
        reason: e.to_string(),
    }
}

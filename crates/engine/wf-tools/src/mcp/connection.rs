//! MCP connection manager: live [`RmcpClient`] instances, the full MCP
//! handshake on connect and the lazy / eager / keep-alive lifecycle modes:
//!
//! - `lazy`: registered only; connected on first use;
//! - `eager`: connected immediately at registration;
//! - `keep-alive`: connected immediately and periodically health-checked.
//!
//! Server configuration and runtime state live in [`super::registry`]; the
//! background maintenance loops live in [`super::health`].

use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::ToolResult;
use crate::mcp::rmcp_client::{McpToolInfo, RmcpClient};
use wf_types::tool::mcp_connection::{McpServerConfig, McpServerStatus};

use super::registry::base_config;
pub use super::registry::{
    server_timeout_ms, McpLifecycleMode, McpServerEntry, McpServerRegistry, DEFAULT_MCP_TIMEOUT_MS,
};

/// Callback type for server connection events
pub type ConnectionCallback = std::sync::RwLock<Arc<dyn Fn(&str) + Send + Sync>>;

#[derive(Clone)]
pub struct McpConnectionManager {
    pub(super) clients: Arc<DashMap<String, Arc<RmcpClient>>>,
    pub(super) registry: Arc<McpServerRegistry>,
    pub(super) last_activity: Arc<DashMap<String, Instant>>,
    /// Shared across clones: all instances observe the same callback.
    on_connected: Arc<ConnectionCallback>,
    /// Invoked when a connected server reports a capability list change
    /// (tools/resources/prompts list-changed); used to refresh metadata.
    on_capabilities_changed: Arc<ConnectionCallback>,
    /// Per-server connect mutex: two concurrent `connect` calls for the same
    /// server must not each spin up a transport (check-then-act race).
    connect_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl McpConnectionManager {
    pub fn new(registry: Arc<McpServerRegistry>) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            registry,
            last_activity: Arc::new(DashMap::new()),
            on_connected: Arc::new(std::sync::RwLock::new(Arc::new(|_| {}))),
            on_capabilities_changed: Arc::new(std::sync::RwLock::new(Arc::new(|_| {}))),
            connect_locks: Arc::new(DashMap::new()),
        }
    }

    /// Register a callback invoked after a server connects and its
    /// capabilities have been discovered (used to register the server's
    /// tools into a shared tool registry).
    pub fn set_on_connected(&self, callback: Arc<dyn Fn(&str) + Send + Sync>) {
        *wf_common::lock::write_ok(self.on_connected.write()) = callback;
    }

    /// Register a callback invoked when a connected server reports a
    /// capability list change (tools/resources/prompts list-changed), so the
    /// caller can re-register or invalidate cached metadata.
    pub fn set_on_capabilities_changed(&self, callback: Arc<dyn Fn(&str) + Send + Sync>) {
        *wf_common::lock::write_ok(self.on_capabilities_changed.write()) = callback;
    }

    pub fn registry(&self) -> &Arc<McpServerRegistry> {
        &self.registry
    }

    /// Register a server without connecting (lazy lifecycle).
    pub fn register_server(&self, name: impl Into<String>, config: McpServerConfig) {
        self.registry.register(name, config);
    }

    /// Register a server and connect according to its lifecycle mode:
    /// lazy = deferred, eager/keep-alive = connect now (keep-alive also
    /// spawns the background health check loop).
    pub async fn connect_server(&self, name: &str, config: McpServerConfig) -> ToolResult<()> {
        let entry = McpServerEntry::new(name.to_string(), config);
        let lifecycle = entry.lifecycle;
        let idle_timeout = entry.idle_timeout;
        let health_check_interval = entry.health_check_interval;
        self.registry.servers.insert(name.to_string(), entry);

        if lifecycle == McpLifecycleMode::Lazy {
            return Ok(());
        }

        self.connect(name).await?;

        if lifecycle == McpLifecycleMode::KeepAlive {
            if let Some(interval) = health_check_interval {
                self.spawn_health_check_loop(name.to_string(), interval);
            }
        }

        // Servers with an idle timeout are disconnected after inactivity.
        // Keep-alive servers are health-checked instead of idle-disconnected.
        if lifecycle != McpLifecycleMode::KeepAlive {
            if let Some(idle) = idle_timeout {
                self.spawn_idle_watcher(name.to_string(), idle);
            }
        }

        Ok(())
    }

    /// Connect to a registered server: full handshake + capability
    /// discovery (tools, resources, resource templates).
    pub async fn connect(&self, server_name: &str) -> ToolResult<()> {
        let entry = self.registry.get(server_name).ok_or_else(|| {
            crate::error::ToolError::NotFound(format!(
                "MCP server '{}' not registered",
                server_name
            ))
        })?;

        let is_disabled = base_config(&entry.config)
            .map(|b| b.disabled == Some(true))
            .unwrap_or(false);
        if is_disabled {
            return Err(crate::error::ToolError::McpError(format!(
                "Server '{}' is disabled",
                server_name
            )));
        }

        // Guard against concurrent connection attempts: serialize per-server
        // and re-check after acquiring the lock (the earlier `is_connected`
        // check alone has a check-then-act race).
        let key = server_name.to_string();
        self.connect_locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
        let connect_lock = self
            .connect_locks
            .get(&key)
            .map(|r| r.clone())
            .expect("connect lock inserted above");
        let guard = connect_lock.lock().await;

        if self.is_connected(server_name) {
            return Ok(());
        }

        self.registry
            .update_status(server_name, McpServerStatus::Connecting);

        // Each server owns its own rmcp-backed client; the handshake and
        // capability discovery are driven inside `RmcpClient::connect`.
        let client = Arc::new(RmcpClient::new(server_name));
        if let Err(e) = client.connect(&entry.config).await {
            self.registry
                .update_status(server_name, McpServerStatus::Disconnected);
            return Err(e);
        }
        self.clients.insert(server_name.to_string(), client.clone());
        self.registry
            .update_status(server_name, McpServerStatus::Connected);
        self.record_activity(server_name);
        drop(guard);

        // Pump server notifications (tool/resource/prompt list-changed) so the
        // manager re-discovers capabilities and refreshes registered tools.
        let pump_client = client.clone();
        let pump_manager = Arc::new(self.clone());
        let on_changed: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |name: &str| {
            let mgr = pump_manager.clone();
            let name = name.to_string();
            tokio::spawn(async move {
                mgr.discover_capabilities(&name).await;
                let cb = wf_common::lock::read_ok(mgr.on_capabilities_changed.read()).clone();
                cb(&name);
            });
        });
        tokio::spawn(async move {
            pump_client.run_notification_pump(on_changed).await;
        });

        self.discover_capabilities(server_name).await;
        let callback = wf_common::lock::read_ok(self.on_connected.read()).clone();
        callback(server_name);
        Ok(())
    }

    pub async fn disconnect(&self, server_name: &str) -> ToolResult<()> {
        if let Some((_, client)) = self.clients.remove(server_name) {
            client.disconnect().await?;
        }
        self.last_activity.remove(server_name);
        self.registry
            .update_status(server_name, McpServerStatus::Disconnected);
        Ok(())
    }

    /// Auto-connect a lazy server before executing an operation on it.
    async fn ensure_connected(&self, server_name: &str) -> ToolResult<()> {
        if self.clients.contains_key(server_name) {
            return Ok(());
        }
        let entry = self.registry.get(server_name).ok_or_else(|| {
            crate::error::ToolError::NotFound(format!(
                "MCP server '{}' not registered",
                server_name
            ))
        })?;
        if entry.status == McpServerStatus::Disconnected
            && base_config(&entry.config)
                .map(|b| b.disabled == Some(true))
                .unwrap_or(false)
        {
            return Err(crate::error::ToolError::McpError(format!(
                "Server '{}' is disabled",
                server_name
            )));
        }
        self.connect(server_name).await
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: &Value,
        timeout_ms: u64,
    ) -> ToolResult<Value> {
        let (server_name, client) = self.find_client_for_tool(tool_name).ok_or_else(|| {
            crate::error::ToolError::NotFound(format!(
                "No connected MCP server provides tool '{}'",
                tool_name
            ))
        })?;
        self.record_activity(&server_name);
        client.call_tool(tool_name, arguments, timeout_ms).await
    }

    pub async fn call_tool_on_server(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: &Value,
        timeout_ms: u64,
    ) -> ToolResult<Value> {
        self.ensure_connected(server_name).await?;
        let client = self.clients.get(server_name).ok_or_else(|| {
            crate::error::ToolError::McpError(format!("Server '{}' not connected", server_name))
        })?;

        if !self.registry.is_tool_allowed(server_name, tool_name) {
            return Err(crate::error::ToolError::McpError(format!(
                "Tool '{}' is not allowed on server '{}'",
                tool_name, server_name
            )));
        }

        self.record_activity(server_name);
        client.call_tool(tool_name, arguments, timeout_ms).await
    }

    pub async fn read_resource(
        &self,
        server_name: &str,
        uri: &str,
        timeout_ms: u64,
    ) -> ToolResult<wf_types::tool::McpResourceReadResult> {
        self.ensure_connected(server_name).await?;
        let client = self.clients.get(server_name).ok_or_else(|| {
            crate::error::ToolError::McpError(format!("Server '{}' not connected", server_name))
        })?;

        self.record_activity(server_name);
        client.read_resource(uri, timeout_ms).await
    }

    pub async fn discover_capabilities(&self, server_name: &str) {
        let _ = self.discover_tools(server_name).await;
        let _ = self.discover_resources(server_name).await;
        let _ = self.discover_resource_templates(server_name).await;
    }

    pub async fn discover_resources(
        &self,
        server_name: &str,
    ) -> ToolResult<Vec<wf_types::tool::McpResource>> {
        let client = self.clients.get(server_name).ok_or_else(|| {
            crate::error::ToolError::McpError(format!("Server '{}' not connected", server_name))
        })?;

        let timeout_ms = self.server_timeout_ms(server_name);
        let resources = client.list_resources(timeout_ms).await?;
        self.registry
            .update_resources(server_name, resources.clone());
        Ok(resources)
    }

    pub async fn discover_resource_templates(
        &self,
        server_name: &str,
    ) -> ToolResult<Vec<wf_types::tool::McpResourceTemplate>> {
        let client = self.clients.get(server_name).ok_or_else(|| {
            crate::error::ToolError::McpError(format!("Server '{}' not connected", server_name))
        })?;

        let timeout_ms = self.server_timeout_ms(server_name);
        let templates = client.list_resource_templates(timeout_ms).await?;
        self.registry
            .update_resource_templates(server_name, templates.clone());
        Ok(templates)
    }

    pub async fn discover_tools(&self, server_name: &str) -> ToolResult<Vec<McpToolInfo>> {
        let client = self.clients.get(server_name).ok_or_else(|| {
            crate::error::ToolError::McpError(format!("Server '{}' not connected", server_name))
        })?;

        let timeout_ms = self.server_timeout_ms(server_name);
        let tools = client.list_tools(timeout_ms).await?;
        self.registry.update_tools(server_name, tools.clone());
        Ok(tools)
    }

    fn server_timeout_ms(&self, server_name: &str) -> u64 {
        self.registry
            .get(server_name)
            .map(|e| server_timeout_ms(&e.config))
            .unwrap_or(DEFAULT_MCP_TIMEOUT_MS)
    }

    pub fn get_client(&self, server_name: &str) -> Option<Arc<RmcpClient>> {
        self.clients.get(server_name).map(|c| c.clone())
    }

    /// Whether a server is currently connected (a live rmcp service exists
    /// and has not been closed).
    pub fn is_connected(&self, server_name: &str) -> bool {
        self.clients
            .get(server_name)
            .map(|c| c.is_connected())
            .unwrap_or(false)
    }

    pub fn connected_servers(&self) -> Vec<String> {
        self.clients
            .iter()
            .filter(|e| {
                self.registry
                    .get(e.key())
                    .map(|s| s.status == McpServerStatus::Connected)
                    .unwrap_or(false)
            })
            .map(|e| e.key().clone())
            .collect()
    }

    /// Register activity on a server (resets the idle timer).
    pub fn record_activity(&self, server_name: &str) {
        self.last_activity
            .insert(server_name.to_string(), Instant::now());
    }

    /// Drop idle servers: those whose configured idle timeout elapsed since
    /// the last activity. Keep-alive servers are never idle-disconnected.
    /// Each removed client is properly disconnected (dispatcher aborted and
    /// transport closed) so its spawned tasks and subprocess do not leak.
    pub async fn disconnect_idle(&self) -> Vec<String> {
        let now = Instant::now();
        let mut disconnected = Vec::new();

        let candidates: Vec<(String, Option<Duration>)> = self
            .registry
            .list()
            .into_iter()
            .filter(|e| e.lifecycle != McpLifecycleMode::KeepAlive)
            .map(|e| (e.name.clone(), e.idle_timeout))
            .collect();

        for (name, idle_timeout) in candidates {
            let Some(idle) = idle_timeout else {
                continue;
            };
            let idle_since = match self.last_activity.get(&name) {
                Some(activity) => *activity.value(),
                None => continue,
            };
            if now.duration_since(idle_since) > idle {
                if let Some((_, client)) = self.clients.remove(&name) {
                    let _ = client.disconnect().await;
                }
                self.registry
                    .update_status(&name, McpServerStatus::Disconnected);
                disconnected.push(name);
            }
        }

        disconnected
    }

    /// Health check one server by listing tools.
    pub async fn check_health(&self, server_name: &str) -> ToolResult<bool> {
        let client = self.clients.get(server_name).ok_or_else(|| {
            crate::error::ToolError::McpError(format!("Server '{}' not connected", server_name))
        })?;

        let timeout_ms = self.server_timeout_ms(server_name);
        match tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            client.list_tools(timeout_ms),
        )
        .await
        {
            Ok(Ok(_)) => {
                self.record_activity(server_name);
                Ok(true)
            }
            _ => {
                self.registry
                    .update_status(server_name, McpServerStatus::Disconnected);
                Ok(false)
            }
        }
    }

    /// Health check all keep-alive servers.
    pub async fn check_all(&self) -> Vec<(String, bool)> {
        let servers: Vec<String> = self
            .registry
            .list()
            .into_iter()
            .filter(|e| {
                e.lifecycle == McpLifecycleMode::KeepAlive && e.status == McpServerStatus::Connected
            })
            .map(|e| e.name)
            .collect();

        let mut results = Vec::new();
        for server in servers {
            let healthy = self.check_health(&server).await.unwrap_or(false);
            if !healthy {
                self.clients.remove(&server);
            }
            results.push((server, healthy));
        }
        results
    }

    /// Reconnect a server after a failed health check.
    pub async fn reconnect(&self, server_name: &str) -> ToolResult<()> {
        if self.clients.contains_key(server_name) {
            let healthy = self.check_health(server_name).await.unwrap_or(false);
            if healthy {
                return Ok(());
            }
        }
        self.clients.remove(server_name);
        self.connect(server_name).await
    }

    fn find_client_for_tool(&self, tool_name: &str) -> Option<(String, Arc<RmcpClient>)> {
        for entry in self.clients.iter() {
            let server_name = entry.key().clone();
            if !self.registry.is_tool_allowed(&server_name, tool_name) {
                continue;
            }
            if let Some(registry_entry) = self.registry.get(&server_name) {
                if registry_entry.tools.iter().any(|t| t.name == tool_name) {
                    return Some((server_name, entry.value().clone()));
                }
            }
        }
        None
    }
}

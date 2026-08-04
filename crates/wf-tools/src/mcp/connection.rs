//! MCP server registry + connection manager.
//!
//! The registry holds server configuration and runtime state (status,
//! discovered tools/resources). The connection manager owns live
//! [`McpClient`] instances, performs the full MCP handshake on connect and
//! supports the lazy / eager / keep-alive lifecycle modes:
//!
//! - `lazy`: registered only; connected on first use;
//! - `eager`: connected immediately at registration;
//! - `keep-alive`: connected immediately and periodically health-checked.
//!
//! [`McpHealthMonitor`] runs the background maintenance (health checks for
//! keep-alive servers, idle disconnects for servers with an idle timeout).

use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::ToolResult;
use crate::mcp::client::{McpClient, McpToolInfo};
use crate::mcp::transport;
use wf_types::tool::mcp_connection::{McpServerConfig, McpServerLifecycle, McpServerStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpLifecycleMode {
    #[default]
    Lazy,
    Eager,
    KeepAlive,
}

impl McpLifecycleMode {
    pub fn from_server_lifecycle(lifecycle: Option<McpServerLifecycle>) -> Self {
        match lifecycle {
            Some(McpServerLifecycle::Eager) => McpLifecycleMode::Eager,
            Some(McpServerLifecycle::KeepAlive) => McpLifecycleMode::KeepAlive,
            _ => McpLifecycleMode::Lazy,
        }
    }
}

/// Default per-call timeout in milliseconds when a server has no configured
/// timeout (TS schema default is 60 seconds).
pub const DEFAULT_MCP_TIMEOUT_MS: u64 = 60_000;

/// Resolve the configured timeout to milliseconds. The configuration value
/// is in seconds (TS schema), matching the `mcp-settings.json` format.
pub fn server_timeout_ms(config: &McpServerConfig) -> u64 {
    let base = match config {
        McpServerConfig::Stdio(c) => &c.base,
        McpServerConfig::Sse(c) => &c.base,
        McpServerConfig::StreamableHttp(c) => &c.base,
    };
    base.timeout.unwrap_or(60).saturating_mul(1000).max(1)
}

#[derive(Debug, Clone)]
pub struct McpServerEntry {
    pub name: String,
    pub config: McpServerConfig,
    pub status: McpServerStatus,
    pub tools: Vec<McpToolInfo>,
    pub resources: Vec<wf_types::tool::McpResource>,
    pub resource_templates: Vec<wf_types::tool::McpResourceTemplate>,
    pub lifecycle: McpLifecycleMode,
    /// Idle disconnect timeout; `None` = never disconnect while idle.
    pub idle_timeout: Option<Duration>,
    /// Health check interval for keep-alive servers.
    pub health_check_interval: Option<Duration>,
}

impl McpServerEntry {
    fn new(name: String, config: McpServerConfig) -> Self {
        let lifecycle =
            McpLifecycleMode::from_server_lifecycle(base_config(&config).and_then(|b| b.lifecycle));
        let idle_timeout = base_config(&config)
            .and_then(|b| b.idle_timeout)
            .map(Duration::from_secs);
        let health_check_interval = base_config(&config)
            .and_then(|b| b.health_check_interval)
            .map(Duration::from_secs);
        Self {
            name,
            config,
            status: McpServerStatus::Disconnected,
            tools: Vec::new(),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            lifecycle,
            idle_timeout,
            health_check_interval,
        }
    }
}

fn base_config(
    config: &McpServerConfig,
) -> Option<&wf_types::tool::mcp_connection::McpServerConfigBase> {
    match config {
        McpServerConfig::Stdio(c) => Some(&c.base),
        McpServerConfig::Sse(c) => Some(&c.base),
        McpServerConfig::StreamableHttp(c) => Some(&c.base),
    }
}

pub struct McpServerRegistry {
    servers: DashMap<String, McpServerEntry>,
}

impl McpServerRegistry {
    pub fn new() -> Self {
        Self {
            servers: DashMap::new(),
        }
    }

    pub fn register(&self, name: impl Into<String>, config: McpServerConfig) {
        let name = name.into();
        self.servers
            .insert(name.clone(), McpServerEntry::new(name, config));
    }

    pub fn unregister(&self, name: &str) -> Option<McpServerEntry> {
        self.servers.remove(name).map(|(_, v)| v)
    }

    pub fn get(&self, name: &str) -> Option<McpServerEntry> {
        self.servers.get(name).map(|e| e.clone())
    }

    pub fn list(&self) -> Vec<McpServerEntry> {
        self.servers.iter().map(|e| e.value().clone()).collect()
    }

    pub fn update_status(&self, name: &str, status: McpServerStatus) {
        if let Some(mut entry) = self.servers.get_mut(name) {
            entry.status = status;
        }
    }

    pub fn update_tools(&self, name: &str, tools: Vec<McpToolInfo>) {
        if let Some(mut entry) = self.servers.get_mut(name) {
            entry.tools = tools;
        }
    }

    pub fn update_resources(&self, name: &str, resources: Vec<wf_types::tool::McpResource>) {
        if let Some(mut entry) = self.servers.get_mut(name) {
            entry.resources = resources;
        }
    }

    pub fn update_resource_templates(
        &self,
        name: &str,
        templates: Vec<wf_types::tool::McpResourceTemplate>,
    ) {
        if let Some(mut entry) = self.servers.get_mut(name) {
            entry.resource_templates = templates;
        }
    }

    pub fn is_disabled(&self, name: &str) -> bool {
        self.servers
            .get(name)
            .map(|e| {
                base_config(&e.config)
                    .map(|b| b.disabled == Some(true))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    pub fn is_tool_allowed(&self, server_name: &str, tool_name: &str) -> bool {
        let Some(entry) = self.servers.get(server_name) else {
            return false;
        };

        let Some(base) = base_config(&entry.config) else {
            return false;
        };

        if base.disabled == Some(true) {
            return false;
        }

        if let Some(disabled_tools) = &base.disabled_tools {
            if disabled_tools.contains(&tool_name.to_string()) {
                return false;
            }
        }

        if let Some(always_allow) = &base.always_allow {
            if always_allow.contains(&tool_name.to_string()) {
                return true;
            }
        }

        true
    }
}

impl Default for McpServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Callback type for server connection events
pub type ConnectionCallback = std::sync::RwLock<Arc<dyn Fn(&str) + Send + Sync>>;

#[derive(Clone)]
pub struct McpConnectionManager {
    clients: Arc<DashMap<String, Arc<McpClient>>>,
    registry: Arc<McpServerRegistry>,
    last_activity: Arc<DashMap<String, Instant>>,
    /// Shared across clones: all instances observe the same callback.
    on_connected: Arc<ConnectionCallback>,
}

impl McpConnectionManager {
    pub fn new(registry: Arc<McpServerRegistry>) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            registry,
            last_activity: Arc::new(DashMap::new()),
            on_connected: Arc::new(std::sync::RwLock::new(Arc::new(|_| {}))),
        }
    }

    /// Register a callback invoked after a server connects and its
    /// capabilities have been discovered (used to register the server's
    /// tools into a shared tool registry).
    pub fn set_on_connected(&self, callback: Arc<dyn Fn(&str) + Send + Sync>) {
        *self.on_connected.write().unwrap() = callback;
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

        // Guard against concurrent connection attempts.
        if self.clients.contains_key(server_name) {
            return Ok(());
        }

        self.registry
            .update_status(server_name, McpServerStatus::Connecting);

        let transport = transport::create_transport(&entry.config);
        let client = McpClient::new(server_name, transport);

        match client.connect().await {
            Ok(_) => {
                self.registry
                    .update_status(server_name, McpServerStatus::Connected);
                self.clients
                    .insert(server_name.to_string(), Arc::new(client));
                self.record_activity(server_name);
                self.discover_capabilities(server_name).await;
                let callback = self.on_connected.read().unwrap().clone();
                callback(server_name);
                Ok(())
            }
            Err(e) => {
                self.registry
                    .update_status(server_name, McpServerStatus::Disconnected);
                Err(e)
            }
        }
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

    pub fn get_client(&self, server_name: &str) -> Option<Arc<McpClient>> {
        self.clients.get(server_name).map(|c| c.clone())
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
    pub fn disconnect_idle(&self) -> Vec<String> {
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
                let _ = self.clients.remove(&name);
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

    /// One maintenance pass: health checks for keep-alive servers and idle
    /// disconnects for servers with an idle timeout.
    pub async fn run_maintenance(&self) {
        let _ = self.check_all().await;
        let _ = self.disconnect_idle();
    }

    /// Spawn the periodic maintenance loop (keep-alive health checks + idle
    /// disconnects). Runs until the returned handle is aborted.
    pub async fn start_maintenance_loop(
        self: Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                self.run_maintenance().await;
            }
        })
    }

    fn spawn_health_check_loop(&self, server_name: String, interval: Duration) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let entry = manager.registry.get(&server_name);
                let keep_alive = entry
                    .map(|e| e.lifecycle == McpLifecycleMode::KeepAlive)
                    .unwrap_or(false);
                if !keep_alive {
                    break;
                }
                if manager.clients.contains_key(&server_name) {
                    let healthy = manager.check_health(&server_name).await.unwrap_or(false);
                    if !healthy {
                        let _ = manager.reconnect(&server_name).await;
                    }
                }
            }
        });
    }

    fn spawn_idle_watcher(&self, server_name: String, idle_timeout: Duration) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(idle_timeout);
            loop {
                ticker.tick().await;
                let entry = manager.registry.get(&server_name);
                let still_eager = entry
                    .map(|e| e.lifecycle == McpLifecycleMode::Eager)
                    .unwrap_or(false);
                if !still_eager {
                    break;
                }
                let idle_since = manager.last_activity.get(&server_name).map(|a| *a.value());
                if let Some(since) = idle_since {
                    if since.elapsed() >= idle_timeout {
                        let _ = manager.disconnect(&server_name).await;
                        break;
                    }
                }
            }
        });
    }

    fn find_client_for_tool(&self, tool_name: &str) -> Option<(String, Arc<McpClient>)> {
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

/// Background health monitor over a connection manager.
pub struct McpHealthMonitor {
    manager: Arc<McpConnectionManager>,
}

impl McpHealthMonitor {
    pub fn new(manager: Arc<McpConnectionManager>) -> Self {
        Self { manager }
    }

    pub fn manager(&self) -> &Arc<McpConnectionManager> {
        &self.manager
    }

    pub fn record_activity(&self, server_name: &str) {
        self.manager.record_activity(server_name);
    }

    pub async fn check_health(&self, server_name: &str) -> ToolResult<bool> {
        self.manager.check_health(server_name).await
    }

    pub async fn check_all(&self) -> Vec<(String, bool)> {
        self.manager.check_all().await
    }

    pub fn disconnect_idle(&self, _idle_timeout: Duration) -> Vec<String> {
        // Idle handling is driven by per-server configured timeouts inside
        // the manager; kept for API compatibility with the previous design.
        self.manager.disconnect_idle()
    }

    pub async fn reconnect(&self, server_name: &str) -> ToolResult<()> {
        self.manager.reconnect(server_name).await
    }

    pub async fn start_health_check_loop(self: Arc<Self>, config: McpHealthCheckConfig) {
        let manager = self.manager.clone();
        let mut interval = tokio::time::interval(config.interval);
        loop {
            interval.tick().await;
            manager.run_maintenance().await;
        }
    }
}

pub struct McpHealthCheckConfig {
    pub interval: Duration,
}

impl Default for McpHealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::tool::mcp_connection::*;

    fn make_stdio_config(command: &str) -> McpServerConfig {
        McpServerConfig::Stdio(McpStdioConfig {
            base: McpServerConfigBase {
                disabled: None,
                timeout: Some(5),
                always_allow: None,
                disabled_tools: None,
                lifecycle: None,
                idle_timeout: None,
                health_check_interval: None,
            },
            command: command.into(),
            args: None,
            cwd: None,
            env: None,
        })
    }

    #[test]
    fn test_register_and_get_server() {
        let registry = McpServerRegistry::new();
        registry.register("test", make_stdio_config("echo"));

        let entry = registry.get("test").unwrap();
        assert_eq!(entry.name, "test");
        assert_eq!(entry.status, McpServerStatus::Disconnected);
        assert_eq!(entry.lifecycle, McpLifecycleMode::Lazy);
    }

    #[test]
    fn test_disabled_server() {
        let registry = McpServerRegistry::new();
        registry.register("test", make_stdio_config("echo"));
        assert!(!registry.is_disabled("test"));

        let mut cfg = make_stdio_config("echo");
        if let McpServerConfig::Stdio(ref mut c) = cfg {
            c.base.disabled = Some(true);
        }
        registry.register("disabled_sv", cfg);
        assert!(registry.is_disabled("disabled_sv"));
    }

    #[test]
    fn test_tool_allowed() {
        let registry = McpServerRegistry::new();
        registry.register("test", make_stdio_config("echo"));
        assert!(registry.is_tool_allowed("test", "any_tool"));

        let mut cfg = make_stdio_config("echo");
        if let McpServerConfig::Stdio(ref mut c) = cfg {
            c.base.disabled_tools = Some(vec!["dangerous_tool".into()]);
        }
        registry.register("restricted", cfg);
        assert!(registry.is_tool_allowed("restricted", "safe_tool"));
        assert!(!registry.is_tool_allowed("restricted", "dangerous_tool"));
    }

    #[test]
    fn test_unregister_server() {
        let registry = McpServerRegistry::new();
        registry.register("test", make_stdio_config("echo"));
        assert!(registry.get("test").is_some());

        registry.unregister("test");
        assert!(registry.get("test").is_none());
    }

    #[test]
    fn test_timeout_is_seconds_based() {
        assert_eq!(server_timeout_ms(&make_stdio_config("echo")), 5000);
        let mut cfg = make_stdio_config("echo");
        if let McpServerConfig::Stdio(ref mut c) = cfg {
            c.base.timeout = None;
        }
        assert_eq!(server_timeout_ms(&cfg), DEFAULT_MCP_TIMEOUT_MS);
    }

    #[test]
    fn test_lifecycle_from_config() {
        let mut cfg = make_stdio_config("echo");
        if let McpServerConfig::Stdio(ref mut c) = cfg {
            c.base.lifecycle = Some(McpServerLifecycle::KeepAlive);
            c.base.health_check_interval = Some(30);
        }
        let entry = McpServerEntry::new("ka".into(), cfg);
        assert_eq!(entry.lifecycle, McpLifecycleMode::KeepAlive);
        assert_eq!(entry.health_check_interval, Some(Duration::from_secs(30)));
    }
}

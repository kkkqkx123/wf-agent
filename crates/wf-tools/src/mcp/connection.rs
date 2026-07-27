use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::ToolResult;
use crate::mcp::client::{McpClient, McpToolInfo};
use crate::mcp::transport;
use wf_types::tool::mcp_connection::{McpServerConfig, McpServerStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpLifecycleMode {
    #[default]
    Lazy,
    Eager,
    KeepAlive,
}

#[derive(Debug, Clone)]
pub struct McpServerEntry {
    pub name: String,
    pub config: McpServerConfig,
    pub status: McpServerStatus,
    pub tools: Vec<McpToolInfo>,
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
        self.servers.insert(
            name.clone(),
            McpServerEntry {
                name,
                config,
                status: McpServerStatus::Disconnected,
                tools: Vec::new(),
            },
        );
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

    pub fn is_disabled(&self, name: &str) -> bool {
        self.servers
            .get(name)
            .map(|e| match &e.config {
                McpServerConfig::Stdio(c) => c.base.disabled == Some(true),
                McpServerConfig::Sse(c) => c.base.disabled == Some(true),
                McpServerConfig::StreamableHttp(c) => c.base.disabled == Some(true),
            })
            .unwrap_or(false)
    }

    pub fn is_tool_allowed(&self, server_name: &str, tool_name: &str) -> bool {
        let Some(entry) = self.servers.get(server_name) else {
            return false;
        };

        let base = match &entry.config {
            McpServerConfig::Stdio(c) => &c.base,
            McpServerConfig::Sse(c) => &c.base,
            McpServerConfig::StreamableHttp(c) => &c.base,
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

pub struct McpConnectionManager {
    clients: DashMap<String, Arc<McpClient>>,
    registry: Arc<McpServerRegistry>,
}

impl McpConnectionManager {
    pub fn new(registry: Arc<McpServerRegistry>) -> Self {
        Self {
            clients: DashMap::new(),
            registry,
        }
    }

    pub async fn connect(&self, server_name: &str) -> ToolResult<()> {
        let entry = self
            .registry
            .get(server_name)
            .ok_or_else(|| crate::error::ToolError::NotFound(format!(
                "MCP server '{}' not registered",
                server_name
            )))?;

        let is_disabled = match &entry.config {
            McpServerConfig::Stdio(c) => c.base.disabled == Some(true),
            McpServerConfig::Sse(c) => c.base.disabled == Some(true),
            McpServerConfig::StreamableHttp(c) => c.base.disabled == Some(true),
        };

        if is_disabled {
            return Err(crate::error::ToolError::McpError(format!(
                "Server '{}' is disabled",
                server_name
            )));
        }

        self.registry
            .update_status(server_name, McpServerStatus::Connecting);

        let transport = transport::create_transport(&entry.config);
        let client = McpClient::new(server_name, transport);

        match client.connect().await {
            Ok(()) => {
                self.registry
                    .update_status(server_name, McpServerStatus::Connected);
                self.clients.insert(server_name.to_string(), Arc::new(client));
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
        self.registry
            .update_status(server_name, McpServerStatus::Disconnected);
        Ok(())
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: &Value,
        timeout_ms: u64,
    ) -> ToolResult<Value> {
        let client = self.find_client_for_tool(tool_name).ok_or_else(|| {
            crate::error::ToolError::NotFound(format!(
                "No connected MCP server provides tool '{}'",
                tool_name
            ))
        })?;

        client.call_tool(tool_name, arguments, timeout_ms).await
    }

    pub async fn discover_tools(&self, server_name: &str) -> ToolResult<Vec<McpToolInfo>> {
        let client = self.clients.get(server_name).ok_or_else(|| {
            crate::error::ToolError::McpError(format!(
                "Server '{}' not connected",
                server_name
            ))
        })?;

        let timeout_ms = self
            .registry
            .get(server_name)
            .and_then(|e| match &e.config {
                McpServerConfig::Stdio(c) => c.base.timeout,
                McpServerConfig::Sse(c) => c.base.timeout,
                McpServerConfig::StreamableHttp(c) => c.base.timeout,
            })
            .unwrap_or(30000);

        let tools = client.list_tools(timeout_ms).await?;
        self.registry.update_tools(server_name, tools.clone());
        Ok(tools)
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

    fn find_client_for_tool(&self, tool_name: &str) -> Option<Arc<McpClient>> {
        for entry in &self.clients {
            let server_name = entry.key();
            if !self.registry.is_tool_allowed(server_name, tool_name) {
                continue;
            }
            if let Some(registry_entry) = self.registry.get(server_name) {
                if registry_entry.tools.iter().any(|t| t.name == tool_name) {
                    return Some(entry.value().clone());
                }
            }
        }
        None
    }
}

pub struct McpHealthCheckConfig {
    pub interval: Duration,
    pub idle_timeout: Option<Duration>,
}

impl Default for McpHealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(300)),
        }
    }
}

pub struct McpHealthMonitor {
    clients: DashMap<String, Arc<McpClient>>,
    registry: Arc<McpServerRegistry>,
    last_activity: DashMap<String, Instant>,
}

impl McpHealthMonitor {
    pub fn new(registry: Arc<McpServerRegistry>, clients: DashMap<String, Arc<McpClient>>) -> Self {
        Self {
            clients,
            registry,
            last_activity: DashMap::new(),
        }
    }

    pub fn record_activity(&self, server_name: &str) {
        self.last_activity.insert(server_name.to_string(), Instant::now());
    }

    pub async fn check_health(&self, server_name: &str) -> ToolResult<bool> {
        let client = self.clients.get(server_name).ok_or_else(|| {
            crate::error::ToolError::McpError(format!("Server '{}' not connected", server_name))
        })?;

        let timeout_ms = self
            .registry
            .get(server_name)
            .and_then(|e| match &e.config {
                McpServerConfig::Stdio(c) => c.base.timeout,
                McpServerConfig::Sse(c) => c.base.timeout,
                McpServerConfig::StreamableHttp(c) => c.base.timeout,
            })
            .unwrap_or(10000);

        match tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            client.list_tools(10000),
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

    pub async fn check_all(&self) -> Vec<(String, bool)> {
        let servers: Vec<String> = self
            .clients
            .iter()
            .filter(|e| {
                self.registry
                    .get(e.key())
                    .map(|s| s.status == McpServerStatus::Connected)
                    .unwrap_or(false)
            })
            .map(|e| e.key().clone())
            .collect();

        let mut results = Vec::new();
        for server in servers {
            let healthy = self.check_health(&server).await.unwrap_or(false);
            results.push((server, healthy));
        }
        results
    }

    pub fn disconnect_idle(&self, idle_timeout: Duration) -> Vec<String> {
        let now = Instant::now();
        let mut disconnected = Vec::new();

        for entry in &self.last_activity {
            if now.duration_since(*entry.value()) > idle_timeout {
                let server_name = entry.key().clone();
                self.clients.remove(&server_name);
                self.registry
                    .update_status(&server_name, McpServerStatus::Disconnected);
                disconnected.push(server_name);
            }
        }

        disconnected
    }

    pub async fn reconnect(&self, server_name: &str) -> ToolResult<()> {
        let entry = self.registry.get(server_name).ok_or_else(|| {
            crate::error::ToolError::NotFound(format!("MCP server '{}' not registered", server_name))
        })?;

        let transport = transport::create_transport(&entry.config);
        let client = McpClient::new(server_name, transport);

        match client.connect().await {
            Ok(()) => {
                self.registry
                    .update_status(server_name, McpServerStatus::Connected);
                self.clients
                    .insert(server_name.to_string(), Arc::new(client));
                self.record_activity(server_name);
                Ok(())
            }
            Err(e) => {
                self.registry
                    .update_status(server_name, McpServerStatus::Disconnected);
                Err(e)
            }
        }
    }

    pub async fn start_health_check_loop(self: Arc<Self>, config: McpHealthCheckConfig) {
        let mut interval = tokio::time::interval(config.interval);

        loop {
            interval.tick().await;

            let _results = self.check_all().await;

            if let Some(idle_timeout) = config.idle_timeout {
                self.disconnect_idle(idle_timeout);
            }
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
                timeout: Some(5000),
                always_allow: None,
                disabled_tools: None,
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
    }

    #[test]
    fn test_disabled_server() {
        let registry = McpServerRegistry::new();
        registry.register("test", make_stdio_config("echo"));
        assert!(!registry.is_disabled("test"));

        let disabled_config = McpServerConfig::Stdio(McpStdioConfig {
            base: McpServerConfigBase {
                disabled: Some(true),
                timeout: Some(5000),
                always_allow: None,
                disabled_tools: None,
            },
            command: "echo".into(),
            args: None,
            cwd: None,
            env: None,
        });
        registry.register("disabled_sv", disabled_config);
        assert!(registry.is_disabled("disabled_sv"));
    }

    #[test]
    fn test_tool_allowed() {
        let registry = McpServerRegistry::new();
        registry.register("test", make_stdio_config("echo"));
        assert!(registry.is_tool_allowed("test", "any_tool"));

        let restricted_config = McpServerConfig::Stdio(McpStdioConfig {
            base: McpServerConfigBase {
                disabled: None,
                timeout: Some(5000),
                always_allow: None,
                disabled_tools: Some(vec!["dangerous_tool".into()]),
            },
            command: "echo".into(),
            args: None,
            cwd: None,
            env: None,
        });
        registry.register("restricted", restricted_config);
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
}

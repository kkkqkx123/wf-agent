//! MCP server registry: configured servers, their lifecycle mode and
//! runtime status/tools discovered by the connection manager.

use dashmap::DashMap;
use std::time::Duration;

use crate::mcp::rmcp_client::McpToolInfo;
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
/// timeout (schema default is 60 seconds).
pub const DEFAULT_MCP_TIMEOUT_MS: u64 = 60_000;

/// Resolve the configured timeout to milliseconds. The configuration value
/// is in seconds, matching the `mcp-settings.json` format.
pub fn server_timeout_ms(config: &McpServerConfig) -> u64 {
    let base = match config {
        McpServerConfig::Stdio(c) => &c.base,
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
    pub(super) fn new(name: String, config: McpServerConfig) -> Self {
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

pub(super) fn base_config(
    config: &McpServerConfig,
) -> Option<&wf_types::tool::mcp_connection::McpServerConfigBase> {
    match config {
        McpServerConfig::Stdio(c) => Some(&c.base),
        McpServerConfig::StreamableHttp(c) => Some(&c.base),
    }
}

pub struct McpServerRegistry {
    pub(super) servers: DashMap<String, McpServerEntry>,
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

        true
    }
}

impl Default for McpServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;

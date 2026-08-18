use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatus {
    Connecting,
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransportType {
    Stdio,
    Sse,
    StreamableHttp,
}

/// Connection lifecycle mode: lazy/eager/keep-alive.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum McpServerLifecycle {
    #[default]
    Lazy,
    Eager,
    KeepAlive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServerConfig {
    Stdio(McpStdioConfig),
    Sse(McpSseConfig),
    #[serde(rename = "streamable-http")]
    StreamableHttp(McpStreamableHttpConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfigBase {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// Per-call timeout in seconds (schema default: 60).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_tools: Option<Vec<String>>,
    /// Connection lifecycle mode; absent means manager default (lazy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<McpServerLifecycle>,
    /// Idle disconnect timeout in seconds (0 = never).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout: Option<u64>,
    /// Health check interval in seconds (keep-alive servers only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_interval: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpStdioConfig {
    #[serde(flatten)]
    pub base: McpServerConfigBase,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpSseConfig {
    #[serde(flatten)]
    pub base: McpServerConfigBase,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpStreamableHttpConfig {
    #[serde(flatten)]
    pub base: McpServerConfigBase,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResourceTemplate {
    pub uri_template: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResourceReadResult {
    pub contents: Vec<McpResourceContent>,
}

/// Top-level MCP settings object, matching the `mcp-settings.json` format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct McpSettings {
    #[serde(rename = "mcpServers", alias = "mcp_servers", default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

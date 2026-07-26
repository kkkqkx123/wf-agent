use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServerConfig {
    Stdio(McpStdioConfig),
    Sse(McpSseConfig),
    StreamableHttp(McpStreamableHttpConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfigBase {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_tools: Option<Vec<String>>,
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

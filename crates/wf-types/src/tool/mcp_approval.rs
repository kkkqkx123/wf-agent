use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpDefaultBehavior {
    AlwaysAsk,
    AlwaysDeny,
    AlwaysApprove,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpApprovalToolConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpApprovalResourceConfig {
    pub uri_pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpApprovalServerConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<McpApprovalToolConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Vec<McpApprovalResourceConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_tool_behavior: Option<McpDefaultBehavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_resource_behavior: Option<McpDefaultBehavior>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpApprovalSettings {
    pub servers: Vec<McpApprovalServerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_server_behavior: Option<McpDefaultBehavior>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpRequestType {
    UseMcp,
    ReadResource,
    ListResources,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpRequest {
    pub r#type: McpRequestType,
    pub server_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpApprovalDecision {
    pub decision: McpDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum McpDecision {
    Approve,
    Deny,
    Ask,
}

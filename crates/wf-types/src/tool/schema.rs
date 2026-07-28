use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolPropertySchema {
    #[serde(rename = "type")]
    pub property_type: String,
    pub description: Option<String>,
    pub items: Option<Box<ToolPropertySchema>>,
    pub properties: Option<std::collections::HashMap<String, ToolPropertySchema>>,
    pub required: Option<Vec<String>>,
    pub r#enum: Option<Vec<Value>>,
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolParametersSchema {
    #[serde(rename = "type")]
    pub parameters_type: String,
    pub properties: std::collections::HashMap<String, ToolPropertySchema>,
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolMetadataSchema {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatelessToolConfigSchema {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<ToolParametersSchema>,
    pub metadata: Option<ToolMetadataSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatefulToolConfigSchema {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<ToolParametersSchema>,
    pub init_params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RestToolConfigSchema {
    pub name: String,
    pub description: Option<String>,
    pub base_url: String,
    pub method: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub parameters: Option<ToolParametersSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuiltinToolConfigSchema {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<ToolParametersSchema>,
    pub handler: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolConfigSchema {
    pub server_name: String,
    pub tool_name: String,
    pub description: Option<String>,
    pub parameters: Option<ToolParametersSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpResourceConfigSchema {
    pub server_name: String,
    pub resource_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalOptionsSchema {
    pub require_confirmation: Option<bool>,
    pub auto_approve: Option<bool>,
    pub risk_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfigSchema {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpApprovalServerConfigSchema {
    pub server_name: String,
    pub require_approval: bool,
    pub auto_approve_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpApprovalSettingsSchema {
    pub default_require_approval: bool,
    pub server_overrides: Option<Vec<McpApprovalServerConfigSchema>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpSettingsSchema {
    pub servers: Option<Vec<McpServerConfigSchema>>,
    pub approval: Option<McpApprovalSettingsSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilePermissionRuleSchema {
    pub path: String,
    pub permission: String,
    pub recursive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilePermissionSettingsSchema {
    pub rules: Vec<FilePermissionRuleSchema>,
    pub default_permission: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceBoundarySettingsSchema {
    pub enabled: bool,
    pub allowed_paths: Option<Vec<String>>,
    pub blocked_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandExecutionSettingsSchema {
    pub timeout: Option<i64>,
    pub max_output_size: Option<u64>,
    pub allowed_commands: Option<Vec<String>>,
    pub blocked_commands: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkSettingsSchema {
    pub enabled: bool,
    pub allowed_hosts: Option<Vec<String>>,
    pub blocked_hosts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractionSettingsSchema {
    pub enabled: bool,
    pub timeout: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategoriesSettingsSchema {
    pub allowed_categories: Option<Vec<String>>,
    pub default_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolConfigSchema {
    pub name: String,
    pub description: Option<String>,
    pub tool_type: String,
    pub parameters: Option<ToolParametersSchema>,
    pub approval: Option<ToolApprovalOptionsSchema>,
    pub metadata: Option<ToolMetadataSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinitionSchema {
    pub name: String,
    pub description: Option<String>,
    pub parameters: ToolParametersSchema,
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSchemaSchema {
    pub definitions: Vec<ToolDefinitionSchema>,
    pub version: Option<String>,
}

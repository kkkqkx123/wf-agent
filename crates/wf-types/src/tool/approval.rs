use serde::{Deserialize, Serialize};

use super::file_permission::FilePermissionSettings;
use super::mcp_approval::McpApprovalSettings;
use super::ToolCall;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityPreset {
    Safe,
    Balanced,
    Permissive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalCategories {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow_read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow_write: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow_execute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow_mcp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow_network: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow_interaction: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceBoundary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_read_only_outside_workspace: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_write_outside_workspace: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandApprovalSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_commands: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denied_commands: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkApprovalSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denied_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractionApprovalSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followup_auto_approve_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_approval_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_preset: Option<SecurityPreset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_threshold: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_approve_patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<ApprovalCategories>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_boundary: Option<WorkspaceBoundary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_permissions: Option<FilePermissionSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandApprovalSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpApprovalSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkApprovalSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction: Option<InteractionApprovalSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_write_protected: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalRequest {
    pub tool_call: ToolCall,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_description: Option<String>,
    pub context_id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub interaction_id: super::super::Id,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalResult {
    pub approved: bool,
    pub tool_call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityPreset {
    Safe,
    Balanced,
    Permissive,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalRequest {
    pub tool_call: super::ToolCall,
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

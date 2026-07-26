use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CrossBoundaryMismatchStrategy {
    Convert,
    Inherit,
    Strict,
    WarnAndContinue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallProtocolConfig {
    pub format: super::ToolCallFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_policy: Option<super::request::ToolCallProtocolViolationPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_protocol: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_cross_boundary_conversion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

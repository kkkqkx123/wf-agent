use serde::{Deserialize, Serialize};

pub use llm_types::tool::definition::{
    Tool, ToolParameterSchema, ToolSchema, DEFAULT_TOOL_TIMEOUT_MS,
};

/// Tool visibility partition for one agent or workflow: which tools are
/// available, initially visible, discoverable, or hidden.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AvailableTools {
    pub available: Vec<String>,
    /// Tools visible in the initial schema. When absent, all `available`
    /// tools are initially visible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial: Option<Vec<String>>,
    /// Discoverable tools: not in the initial schema; only metadata is
    /// injected into the prompt and calls go through the `general` tool.
    /// Schema injection happens only when the tool is activated via
    /// TOOL_VISIBILITY unblock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discoverable: Option<Vec<String>>,
    /// Explicitly hidden tools: registered but never exposed to the model
    /// (supplements runtime visibility blocking).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<Vec<String>>,
    /// Escape hatch controlling whether the `general` tool is exposed.
    /// Defaults to auto: exposed iff the discoverable list is non-empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_general_tool: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_workflows: Option<Vec<String>>,
}

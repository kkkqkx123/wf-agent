use serde::{Deserialize, Serialize};

use llm_types::tool::definition::ToolParameterSchema;

/// Last-resort wall-clock bound (milliseconds) for a single tool execution
/// when neither the tool definition's `default_timeout_ms` nor an explicit
/// `ToolExecutionOptions::timeout` supplies a value. Shared so the executor
/// wrapper and every caller agree on one fallback rather than repeating the
/// literal.
pub const DEFAULT_TOOL_TIMEOUT_MS: u64 = 30_000;

/// Rich agent-side tool definition: the wire declaration plus registry
/// identity, kind, governance metadata and execution knobs. Only the
/// name/description/parameters travel to providers, via `wire_declaration`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub tool_type: super::ToolType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<ToolParameterSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::ToolMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_timeout_ms: Option<u64>,
}

impl Tool {
    /// Project the provider-facing wire declaration carried on `LlmRequest`.
    pub fn wire_declaration(&self) -> llm_types::tool::Tool {
        llm_types::tool::Tool {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
        }
    }
}

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

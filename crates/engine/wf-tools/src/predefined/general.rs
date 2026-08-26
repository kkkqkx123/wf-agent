//! Definition of the general tool (builtin type).
//!
//! `general` is the invoke proxy for tools that are not in the initial
//! schema (discoverable tools). The schema is deliberately minimal and
//! fixed: a single string parameter with no inner schema constraints, so
//! the LLM-facing schema never changes when the discoverable set changes
//! (KV-cache friendly).

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

/// All general-category tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&GENERAL];

pub static GENERAL: ToolDefinition = ToolDefinition {
    id: "general",
    tool_type: ToolType::BuiltIn,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "utility",
    tags: &["general", "discovery"],
    description: "Invoke tools whose schemas are not directly exposed. The request body is a JSON \
                  object {\"tool\": \"tool_name\", \"parameters\": {...}} wrapped in a regular \
                  <tool_use> call, e.g.:\n\
                  <tool_use>\n  <tool_name>general</tool_name>\n  <parameters>\n    \
                  <request>{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust\"}}</request>\n  \
                  </parameters>\n</tool_use>\n\
                  The inner tool is interpreted and executed server-side.",
    parameters: &[
        ToolParameter {
            name: "request",
            r#type: "string",
            required: true,
            description: "The inner tool invocation as a JSON object {\"tool\": \"name\", \"parameters\": {...}} (or an array of such objects for multiple calls).",
            default_json: None,
            constraints: None,
        },
    ],
    tips: None,
    examples: None,
};

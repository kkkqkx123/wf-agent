//! Predefined knowledge/integration tools: definitions only. `skill` is
//! executed by the BuiltinExecutor; `use_mcp` by the McpExecutor.

pub mod skill;
pub mod use_mcp;

pub use skill::SKILL;
pub use use_mcp::USE_MCP;

use super::schema::ToolDefinition;

/// All knowledge tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&SKILL, &USE_MCP];

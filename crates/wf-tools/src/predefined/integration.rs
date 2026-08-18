//! Predefined integration tools: definitions only. `use_mcp` is executed by
//! the McpExecutor.

pub mod use_mcp;

pub use use_mcp::USE_MCP;

use super::schema::ToolDefinition;

/// All integration tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&USE_MCP];

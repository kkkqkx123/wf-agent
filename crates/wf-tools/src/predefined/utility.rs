//! Predefined utility tools: definitions + handler.
//!
//! Tools: update_todo_list. Each tool lives in its own file.

pub mod update_todo_list;

pub use update_todo_list::UPDATE_TODO_LIST;

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;

/// All utility tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&UPDATE_TODO_LIST];

/// Register the utility tool handlers into the registry.
pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    update_todo_list::register(registry)?;
    Ok(())
}

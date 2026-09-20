//! Predefined knowledge tools. `skill` is executed by the
//! BuiltinExecutor; `register` installs its always-available definition
//! (content loading is served by the injected skill loader).

pub mod skill;

pub use skill::SKILL;

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;

/// All knowledge tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&SKILL];

/// Register the `skill` definition. There is no handler to wire: execution
/// goes through the BuiltinExecutor.
pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    registry.register_tool(SKILL.tool_def());
    Ok(())
}

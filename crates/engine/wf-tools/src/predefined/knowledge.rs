//! Predefined knowledge tools: definitions only. `skill` is executed by the
//! BuiltinExecutor.

pub mod skill;

pub use skill::SKILL;

use super::schema::ToolDefinition;

/// All knowledge tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&SKILL];

//! Predefined agent tools (builtin type): definitions only. Execution is
//! handled by the BuiltinExecutor through the registered ExecutionCallback.

pub mod call_agent;

pub use call_agent::CALL_AGENT;

use super::schema::ToolDefinition;

/// All agent tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&CALL_AGENT];

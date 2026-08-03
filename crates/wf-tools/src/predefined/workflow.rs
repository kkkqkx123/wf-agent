//! Predefined workflow tools (builtin type): definitions only. Execution is
//! handled by the BuiltinExecutor through the registered ExecutionCallback.

pub mod cancel_workflow;
pub mod execute_workflow;
pub mod query_workflow_status;

pub use cancel_workflow::CANCEL_WORKFLOW;
pub use execute_workflow::EXECUTE_WORKFLOW;
pub use query_workflow_status::QUERY_WORKFLOW_STATUS;

use super::schema::ToolDefinition;

/// All workflow tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&EXECUTE_WORKFLOW, &QUERY_WORKFLOW_STATUS, &CANCEL_WORKFLOW];

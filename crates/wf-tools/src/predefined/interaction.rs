//! Predefined interaction tools (builtin type): definitions only. These
//! signal the agent loop to pause for user input or to complete the task.

pub mod ask_followup_question;
pub mod attempt_completion;

pub use ask_followup_question::ASK_FOLLOWUP_QUESTION;
pub use attempt_completion::ATTEMPT_COMPLETION;

use super::schema::ToolDefinition;

/// All interaction tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&ASK_FOLLOWUP_QUESTION, &ATTEMPT_COMPLETION];

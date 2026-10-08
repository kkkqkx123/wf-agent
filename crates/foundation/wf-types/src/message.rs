//! Conversation message models, re-exported from `llm-types`, plus the
//! agent-side message collections and operations.
//!
//! The canonical message and tool-call definitions live in the shared
//! `llm-types` crate; the view/operation types below stay here because they
//! describe the agent conversation surface.

pub use llm_types::message::{
    ImageUrlContent, LlmFunctionCall, LlmToolCall, Message, MessageContent, MessageContentValue,
    MessageRole, ToolResultContent, ToolUseContent,
};

pub mod batch_management_operation;
pub mod batch_snapshot;
pub mod message_array;
pub mod message_context;
pub mod message_mark_map;
pub mod message_operations;
pub mod message_view;
pub mod named_message_context;

pub use batch_management_operation::*;
pub use batch_snapshot::*;
pub use message_array::*;
pub use message_context::*;
pub use message_mark_map::*;
pub use message_operations::*;
pub use message_view::*;
pub use named_message_context::*;

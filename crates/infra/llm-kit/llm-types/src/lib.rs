//! Shared data models for the llm-kit crates.
//!
//! This crate is a dependency-free protocol leaf: messages, LLM request and
//! response envelopes, wire tool declarations, parameter schemas and usage
//! observability shared by provider crates and host projects.
//! Agent-side governance (tool identity, kind, metadata, exposure, risk,
//! checkpoint policy) and execution state (token ledgers, execution configs)
//! belong to the host workspace and never live here, so this crate stays
//! independently publishable. Protocol-specific logic lives in
//! `llm-tool-call`, transport in the provider crates.

pub mod common;
pub mod error;
pub mod llm;
pub mod message;
pub mod tool;

pub use common::{Id, Metadata, Timestamp};
pub use error::{Error, Result};
pub use message::{
    ImageUrlContent, LlmFunctionCall, LlmToolCall, Message, MessageContent, MessageContentValue,
    MessageRole, ToolResultContent, ToolUseContent,
};

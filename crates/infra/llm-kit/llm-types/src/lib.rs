//! Shared data models for the llm-kit crates.
//!
//! This crate is a leaf: it carries stable types (messages, LLM request and
//! response envelopes, tool schemas, configs, errors) that provider crates
//! and host projects align on. Protocol-specific logic lives in
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

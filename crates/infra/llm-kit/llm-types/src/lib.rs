//! Shared data models for the llm-kit crates.
//!
//! This crate is a leaf: it carries stable types (messages, configs, errors)
//! that provider crates and host projects align on. Protocol-specific logic
//! lives in `llm-tool-call`, transport in the provider crates.

pub mod error;
pub mod message;

pub use error::{Error, Result};
pub use message::{ContentPart, Message, MessageContent, MessageRole, ToolCall, ToolCallDelta};

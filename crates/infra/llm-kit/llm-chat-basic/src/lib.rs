//! Basic OpenAI-compatible chat HTTP client for the llm-kit group.
//!
//! Ported from the code-context-engine `cce-llm-client` chat handler: plain
//! `/chat/completions` calls without protocol codecs, tool-call handling, or
//! agent-loop machinery (those stay in `wf-llm`).
//!
//! Optional capabilities are feature-gated so simple callers pay nothing:
//! - `streaming`: server-sent-events delta stream;
//! - `retry`: exponential-backoff retry around chat calls;
//! - `ratelimit`: token-bucket rate limiting.

pub mod client;
pub mod config;
pub mod error;
pub mod request_builder;

#[cfg(feature = "ratelimit")]
pub mod ratelimit;
#[cfg(feature = "retry")]
pub mod retry;
#[cfg(feature = "streaming")]
pub mod stream;

pub use client::{BasicChatClient, ChatResult, LlmChat};
pub use config::{ChatConfig, ResponseFormat};
pub use error::{ChatError, Result};
#[cfg(feature = "ratelimit")]
pub use ratelimit::RateLimiter;
pub use request_builder::RequestBuilder;
#[cfg(feature = "retry")]
pub use retry::RetryPolicy;
#[cfg(feature = "streaming")]
pub use stream::{ChatEventStream, ChatStreamEvent};

//! Embedding capability for the llm-kit group.
//!
//! Provides a provider trait plus an OpenAI-compatible HTTP implementation
//! covering every OpenAI-compatible endpoint (OpenAI, Gemini, Azure, Ollama).
//! Query preprocessing (Nomic/Stella task prefixes) is ported from linkrs.
//!
//! This crate intentionally has no feature flags: the HTTP call and provider
//! presets carry little extra complexity, so a single implementation path is
//! preferred. Token-budget batching stays out on purpose: it needs a
//! tokenizer that would couple this leaf crate to engine code.

pub mod config;
pub mod error;
pub mod openai_compatible;
pub mod preprocessor;
pub mod provider;
pub mod service;

pub use config::EmbeddingConfig;
pub use error::{EmbeddingError, Result};
pub use openai_compatible::OpenAICompatibleProvider;
pub use preprocessor::{PreprocessorConfig, PreprocessorImpl};
pub use provider::{EmbeddingProvider, EmbeddingResult};
pub use service::EmbeddingService;

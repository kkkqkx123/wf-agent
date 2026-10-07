//! Rerank capability for the llm-kit group.
//!
//! Two providers ported from code-context-engine `cce-llm-client`: a
//! Cohere-compatible dedicated `/rerank` endpoint and a generative provider
//! that scores candidates through an OpenAI-compatible chat prompt. Both
//! share the candidate types and score-fusion strategy below.
//!
//! The crate intentionally has no feature flags: the two transports are
//! small, and callers select the provider at runtime, not compile time.

pub mod cohere;
pub mod config;
pub mod error;
pub mod fusion;
pub mod generative;
pub mod provider;

pub use cohere::CohereRerankProvider;
pub use config::RerankConfig;
pub use error::{RerankError, Result};
pub use fusion::RerankFusionStrategy;
pub use generative::{GenerativeChatEndpoint, GenerativeRerankProvider};
pub use provider::{
    RerankCandidate, RerankProvider, RerankRequest, RerankResult, RerankRuntimeConfig,
    RerankedCandidate,
};

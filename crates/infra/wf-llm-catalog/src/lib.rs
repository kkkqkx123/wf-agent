//! Built-in LLM catalog.
//!
//! This crate is the single home for two pieces of static data the runtime
//! needs before any file-layer or plugin configuration is loaded:
//!
//! 1. **Provider connection templates** (`providers` module): base URL,
//!    wire format, auth type, discovery mode and narrow compat defaults
//!    for the well-known providers.
//! 2. **Model metadata** (`models` module): context window, modality and
//!    reasoning flags derived from the embedded LiteLLM table.
//!
//! Keeping both here, instead of splitting them between `wf-llm` and
//! `wf-pricing`, gives hosts one import for "what is built in" and keeps
//! the heavy pricing table behind a thin query layer. `wf-llm` re-exports
//! the provider templates directly, so there is no separate `builtin`
//! module there anymore.

pub mod models;
pub mod providers;

pub use models::{model_metadata, model_metadata_for, ModelMetadataResolver};
pub use providers::{
    builtin_provider_definitions, builtin_provider_ids, register_builtin_providers,
    BUILTIN_ANTHROPIC_ID, BUILTIN_GEMINI_ID, BUILTIN_OLLAMA_ID, BUILTIN_OPENAI_ID,
    BUILTIN_OPENROUTER_ID,
};
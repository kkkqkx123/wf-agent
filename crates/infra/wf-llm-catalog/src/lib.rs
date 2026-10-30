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
    BUILTIN_AMAZON_BEDROCK_ID, BUILTIN_ANTHROPIC_ID, BUILTIN_ANT_LING_ID, BUILTIN_AZURE_ID,
    BUILTIN_BASETEN_ID, BUILTIN_CEREBRAS_ID, BUILTIN_CLOUDFLARE_AI_GATEWAY_ID,
    BUILTIN_CLOUDFLARE_WORKERS_AI_ID, BUILTIN_DEEPSEEK_ID, BUILTIN_FIREWORKS_ID, BUILTIN_GEMINI_ID,
    BUILTIN_GITHUB_COPILOT_ID, BUILTIN_GOOGLE_VERTEX_ID, BUILTIN_GROQ_ID, BUILTIN_HUGGINGFACE_ID,
    BUILTIN_KIMI_CODING_ID, BUILTIN_META_ID, BUILTIN_MINIMAX_CN_ID, BUILTIN_MINIMAX_ID,
    BUILTIN_MISTRAL_ID, BUILTIN_MOONSHOTAI_CN_ID, BUILTIN_MOONSHOTAI_ID, BUILTIN_NVIDIA_ID,
    BUILTIN_OLLAMA_ID, BUILTIN_OPENAI_CODEX_ID, BUILTIN_OPENAI_ID, BUILTIN_OPENAI_RESPONSES_ID,
    BUILTIN_OPENCODE_GO_ID, BUILTIN_OPENCODE_ID, BUILTIN_OPENROUTER_ID,
    BUILTIN_QWEN_TOKEN_PLAN_CN_ID, BUILTIN_QWEN_TOKEN_PLAN_ID,
    BUILTIN_QWEN_TOKEN_PLAN_INDIVIDUAL_ID, BUILTIN_TOGETHER_ID, BUILTIN_VERCEL_AI_GATEWAY_ID,
    BUILTIN_XAI_ID, BUILTIN_XIAOMI_ID, BUILTIN_XIAOMI_TOKEN_PLAN_AMS_ID,
    BUILTIN_XIAOMI_TOKEN_PLAN_CN_ID, BUILTIN_XIAOMI_TOKEN_PLAN_SGP_ID, BUILTIN_ZAI_CODING_CN_ID,
    BUILTIN_ZAI_ID, OPENCODE_CLIENT_HEADER, OPENCODE_CLIENT_NAME, OPENCODE_SESSION_HEADER,
};

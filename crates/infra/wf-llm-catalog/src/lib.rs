//! Built-in LLM catalog.
//!
//! This crate is the single home for two pieces of static data the runtime
//! needs before any file-layer or plugin configuration is loaded:
//!
//! 1. **Provider connection templates** (`providers` module): base URL,
//!    wire format, auth type, discovery mode and narrow compat defaults
//!    for the well-known providers.
//! 2. **Model metadata**: context window, modality and reasoning flags
//!    derived from the embedded LiteLLM table.
//!
//! Keeping both here, instead of splitting them between `wf-llm` and
//! `wf-pricing`, gives hosts one import for "what is built in" and keeps
//! the heavy pricing table behind a thin query layer. `wf-llm` re-exports
//! the provider templates directly, so there is no separate `builtin`
//! module there anymore.

pub mod providers;

pub use wf_pricing::ModelMetadataResolver;

use std::collections::HashMap;
use std::sync::Mutex;

use llm_types::llm::ModelMetadata;

/// Cache of metadata for models that have actually been queried.
///
/// This intentionally does **not** retain the full embedded LiteLLM table:
/// only models reached through runtime/profile lookups are materialized, so
/// resident memory stays proportional to configured usage rather than to the
/// whole (multi-megabyte) embedded table. The table is parsed on demand when
/// a model is missing from the cache; that refresh cost is acceptable
/// because misses are rare after warm-up.
static MODEL_METADATA_CACHE: Mutex<Option<HashMap<String, Option<ModelMetadata>>>> =
    Mutex::new(None);

fn cache() -> std::sync::MutexGuard<'static, Option<HashMap<String, Option<ModelMetadata>>>> {
    let mut guard = MODEL_METADATA_CACHE.lock().expect("model metadata cache lock");
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

/// Metadata for one model name, if the embedded table knows it.
///
/// Results are cached by model name. Unknown models are cached as `None` so
/// repeated lookups for a missing model do not re-parse the embedded table.
pub fn model_metadata(model: &str) -> Option<ModelMetadata> {
    {
        let cache = cache();
        if let Some(value) = cache.as_ref().and_then(|m| m.get(model)) {
            return value.clone();
        }
    }

    let value = ModelMetadataResolver::from_embedded().metadata_for(model);

    let mut cache = cache();
    if let Some(map) = cache.as_mut() {
        map.insert(model.to_string(), value.clone());
    }
    value
}

/// Metadata for every model in `models`, in input order.
///
/// Unknown models are skipped, matching the historical behavior. All misses
/// are resolved with a single parse of the embedded table, and results are
/// stored in the shared cache, so callers processing a configuration file
/// should prefer this over repeated `model_metadata` calls.
pub fn model_metadata_for(models: &[&str]) -> Vec<ModelMetadata> {
    let mut missing: Vec<&str> = Vec::new();
    {
        let cache = cache();
        if let Some(map) = cache.as_ref() {
            for model in models {
                if !map.contains_key(*model) && !missing.contains(model) {
                    missing.push(*model);
                }
            }
        }
    }

    if !missing.is_empty() {
        let resolver = ModelMetadataResolver::from_embedded();
        let mut cache = cache();
        if let Some(map) = cache.as_mut() {
            for model in missing {
                map.insert(model.to_string(), resolver.metadata_for(model));
            }
        }
    }

    models
        .iter()
        .filter_map(|model| {
            let cache = cache();
            cache.as_ref()?.get(*model).cloned().flatten()
        })
        .collect()
}

/// Map a local provider id to the LiteLLM provider namespace used by the
/// embedded pricing/metadata table, when known.
///
/// Built-in ids map to their upstream LiteLLM provider name. Custom
/// providers should expose the same value through their definition metadata
/// under the `litellm_provider` key; callers can resolve that separately.
pub fn litellm_provider_for(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "builtin-openai" | "builtin-openai-responses" | "builtin-openai-codex" => Some("openai"),
        "builtin-anthropic" => Some("anthropic"),
        "builtin-gemini" | "builtin-google-vertex" => Some("gemini"),
        "builtin-ollama" => Some("ollama"),
        "builtin-openrouter" => Some("openrouter"),
        "builtin-deepseek" => Some("deepseek"),
        "builtin-groq" => Some("groq"),
        "builtin-together" => Some("together_ai"),
        "builtin-fireworks" => Some("fireworks_ai"),
        "builtin-cerebras" => Some("cerebras"),
        "builtin-baseten" => Some("baseten"),
        "builtin-nvidia" => Some("nvidia_nim"),
        "builtin-moonshotai" | "builtin-moonshotai-cn" | "builtin-kimi-coding" => Some("moonshot"),
        "builtin-zai" | "builtin-zai-coding-cn" => Some("zai"),
        "builtin-xiaomi"
        | "builtin-xiaomi-token-plan-ams"
        | "builtin-xiaomi-token-plan-cn"
        | "builtin-xiaomi-token-plan-sgp" => Some("xiaomi_mimo"),
        "builtin-qwen-token-plan"
        | "builtin-qwen-token-plan-cn"
        | "builtin-qwen-token-plan-individual" => Some("dashscope"),
        "builtin-github-copilot" => Some("github_copilot"),
        "builtin-mistral" => Some("mistral"),
        "builtin-opencode" | "builtin-opencode-go" => Some("openai"),
        "builtin-azure" => Some("azure"),
        "builtin-cloudflare-ai-gateway" | "builtin-cloudflare-workers-ai" => Some("cloudflare"),
        "builtin-minimax" | "builtin-minimax-cn" => Some("minimax"),
        "builtin-vercel-ai-gateway" => Some("vercel_ai_gateway"),
        "builtin-meta" => Some("meta"),
        "builtin-xai" => Some("xai"),
        "builtin-amazon-bedrock" => Some("bedrock_converse"),
        "builtin-ant-ling" => None,
        "builtin-huggingface" => None,
        _ => None,
    }
}

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

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
    let mut guard = MODEL_METADATA_CACHE
        .lock()
        .expect("model metadata cache lock");
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

/// Metadata for one model with an optional local provider id.
///
/// The provider id is translated through [`litellm_provider_for`] and the
/// provider-qualified table key is tried before the canonical short name,
/// so gateway-hosted models without a canonical entry still resolve.
/// Results share the same process cache as [`model_metadata`], keyed by
/// the qualified name when a mapping exists. Provider ids unknown to the
/// built-in mapping fall back to the canonical short name; callers that
/// resolved a custom `litellm_provider` declaration should use
/// [`model_metadata_for_namespace`] directly.
pub fn model_metadata_for_provider(
    provider_id: Option<&str>,
    model: &str,
) -> Option<ModelMetadata> {
    let namespace = if model.contains('/') {
        None
    } else {
        provider_id.and_then(litellm_provider_for)
    };
    model_metadata_for_namespace(namespace, model)
}

/// Metadata for one model with an already-resolved LiteLLM namespace.
///
/// Lower-level counterpart of [`model_metadata_for_provider`] for callers
/// holding a custom mapping (such as file-layer `litellm_provider`
/// declarations): the namespace is used as given, with no built-in
/// translation. Cache keys match the provider-based entry, so both paths
/// share entries for the same effective namespace.
pub fn model_metadata_for_namespace(
    litellm_provider: Option<&str>,
    model: &str,
) -> Option<ModelMetadata> {
    let key = cache_key_for_namespace(litellm_provider, model);
    {
        let cache = cache();
        if let Some(value) = cache.as_ref().and_then(|m| m.get(&key)) {
            return value.clone();
        }
    }
    let value = fetch_uncached_with(litellm_provider, model);
    let mut cache = cache();
    if let Some(map) = cache.as_mut() {
        map.insert(key, value.clone());
    }
    value
}

/// Cache key for a namespace-resolved lookup.
fn cache_key_for_namespace(litellm_provider: Option<&str>, model: &str) -> String {
    if model.contains('/') {
        return model.to_string();
    }
    match litellm_provider {
        Some(namespace) => format!("{namespace}/{model}"),
        None => model.to_string(),
    }
}

/// Resolve one namespace-resolved lookup with a fresh table parse.
fn fetch_uncached_with(litellm_provider: Option<&str>, model: &str) -> Option<ModelMetadata> {
    ModelMetadataResolver::from_embedded().metadata_for_provider(litellm_provider, model)
}

/// Metadata for every `(provider, model)` pair in input order.
///
/// Provider-aware counterpart of [`model_metadata_for`]: unknown models
/// are skipped and all misses share a single parse of the embedded table.
/// Callers processing a configuration file should prefer this over
/// repeated [`model_metadata_for_provider`] calls.
pub fn model_metadata_for_providers(pairs: &[(Option<&str>, &str)]) -> Vec<ModelMetadata> {
    let resolved: Vec<(Option<&str>, &str)> = pairs
        .iter()
        .map(|(provider_id, model)| {
            let namespace = if model.contains('/') {
                None
            } else {
                provider_id.and_then(litellm_provider_for)
            };
            (namespace, *model)
        })
        .collect();
    model_metadata_for_namespaces(&resolved)
}

/// Metadata for every `(namespace, model)` pair in input order.
///
/// Batch counterpart of [`model_metadata_for_namespace`] for callers
/// holding a custom mapping: namespaces are used as given, unknown models
/// are skipped, and all misses share a single parse of the embedded table.
pub fn model_metadata_for_namespaces(pairs: &[(Option<&str>, &str)]) -> Vec<ModelMetadata> {
    let mut missing: Vec<(String, Option<String>, String)> = Vec::new();
    {
        let cache = cache();
        if let Some(map) = cache.as_ref() {
            for (namespace, model) in pairs {
                let key = cache_key_for_namespace(*namespace, model);
                if !map.contains_key(&key) && !missing.iter().any(|(k, _, _)| k == &key) {
                    missing.push((key, namespace.map(str::to_string), model.to_string()));
                }
            }
        }
    }

    if !missing.is_empty() {
        let resolver = ModelMetadataResolver::from_embedded();
        let mut cache = cache();
        if let Some(map) = cache.as_mut() {
            for (key, namespace, model) in missing {
                map.insert(
                    key,
                    resolver.metadata_for_provider(namespace.as_deref(), &model),
                );
            }
        }
    }

    pairs
        .iter()
        .filter_map(|(namespace, model)| {
            let cache = cache();
            let key = cache_key_for_namespace(*namespace, model);
            cache.as_ref()?.get(&key).cloned().flatten()
        })
        .collect()
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

/// Extract a custom `litellm_provider` declaration from a provider definition.
///
/// Returns the declared upstream namespace, if any. No validation is done
/// here: unknown namespaces simply miss the qualified lookup and fall back
/// to the canonical entry downstream.
pub fn custom_litellm_of(definition: &llm_types::llm::LlmProviderDefinition) -> Option<&str> {
    definition
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("litellm_provider"))
        .and_then(|value| value.as_str())
}

/// Resolve the effective LiteLLM namespace for a local provider id.
///
/// An explicit custom declaration always wins over the built-in mapping,
/// matching the file-layer-wins registration order.
pub fn resolve_litellm_namespace<'a>(
    provider_id: &str,
    custom: Option<&'a str>,
) -> Option<&'a str> {
    custom.or_else(|| litellm_provider_for(provider_id))
}

/// Assemble the local-id to LiteLLM-namespace mapping for pricing and
/// metadata lookups.
///
/// Built-in ids contribute their upstream namespace first; file-layer
/// definitions then override by id when they declare `litellm_provider`.
/// A definition that overrides an id without a declaration keeps the
/// built-in mapping, since such overrides normally only adjust connection
/// fields while staying with the same vendor.
pub fn provider_mapping_for(
    definitions: &[llm_types::llm::LlmProviderDefinition],
) -> HashMap<String, String> {
    let mut mapping = HashMap::new();
    for id in builtin_provider_ids() {
        if let Some(namespace) = litellm_provider_for(&id) {
            mapping.insert(id, namespace.to_string());
        }
    }
    for definition in definitions {
        if let Some(namespace) = custom_litellm_of(definition) {
            mapping.insert(definition.id.clone(), namespace.to_string());
        }
    }
    mapping
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
        "builtin-gemini" => Some("gemini"),
        // Vertex serves its own model family under `vertex_ai/*`; sharing
        // the Gemini namespace would miss vertex-only models.
        "builtin-google-vertex" => Some("vertex_ai"),
        "builtin-ollama" => Some("ollama"),
        // The embedded table tracks no openrouter-specific entries today;
        // kept so a future table with `openrouter/*` keys resolves
        // without a code change. Canonical fallback covers current models.
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
        // Bedrock entries in the embedded table use unprefixed model ids,
        // so this mapping currently misses and the canonical fallback
        // covers them. Kept for forward compatibility with qualified keys.
        "builtin-amazon-bedrock" => Some("bedrock_converse"),
        "builtin-ant-ling" => None,
        "builtin-huggingface" => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_maps_to_vertex_namespace() {
        assert_eq!(litellm_provider_for("builtin-gemini"), Some("gemini"));
        assert_eq!(
            litellm_provider_for("builtin-google-vertex"),
            Some("vertex_ai")
        );
        assert_eq!(litellm_provider_for("builtin-huggingface"), None);
    }

    #[test]
    fn provider_aware_lookup_resolves_gateway_models() {
        // No canonical short name exists for this model.
        assert!(model_metadata("codestral-latest").is_none());
        let meta = model_metadata_for_provider(Some("builtin-mistral"), "codestral-latest")
            .expect("mistral qualified entry");
        assert_eq!(meta.id, "codestral-latest");
        assert!(meta.context_window.is_some());
        // Canonical models resolve with or without a provider scope.
        assert!(model_metadata_for_provider(Some("builtin-openai"), "gpt-4o").is_some());
        assert!(model_metadata_for_provider(None, "gpt-4o").is_some());
        assert!(model_metadata_for_provider(None, "no-such-model-xyz").is_none());
    }

    fn test_definition(
        id: &str,
        litellm_provider: Option<&str>,
    ) -> llm_types::llm::LlmProviderDefinition {
        llm_types::llm::LlmProviderDefinition {
            id: id.to_string(),
            name: None,
            description: None,
            base_url: None,
            auth_type: None,
            default_headers: None,
            format: llm_types::llm::LlmFormat::OpenaiChat,
            model_discovery: None,
            api_version: None,
            metadata: litellm_provider.map(|namespace| {
                [("litellm_provider".to_string(), serde_json::json!(namespace))]
                    .into_iter()
                    .collect()
            }),
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        }
    }

    #[test]
    fn mapping_assembly_prefers_custom_declarations() {
        let definitions = vec![
            test_definition("my-mistral-proxy", Some("mistral")),
            test_definition("builtin-azure", Some("custom_azure")),
            test_definition("plain-proxy", None),
        ];
        let mapping = provider_mapping_for(&definitions);
        // Custom-only id gains a namespace.
        assert_eq!(
            mapping.get("my-mistral-proxy").map(String::as_str),
            Some("mistral")
        );
        // Explicit declaration wins over the built-in mapping.
        assert_eq!(
            mapping.get("builtin-azure").map(String::as_str),
            Some("custom_azure")
        );
        // No declaration leaves the id unmapped.
        assert!(!mapping.contains_key("plain-proxy"));
        // Untouched built-ins keep their mapping.
        assert_eq!(
            mapping.get("builtin-mistral").map(String::as_str),
            Some("mistral")
        );
    }

    #[test]
    fn namespace_lookup_serves_custom_providers() {
        // A custom proxy id has no built-in mapping, so the provider-based
        // entry falls back to the canonical short name and misses.
        assert!(
            model_metadata_for_provider(Some("my-mistral-proxy"), "codestral-latest").is_none()
        );
        // With the declared namespace the qualified entry resolves and the
        // id echoes the requested short name.
        let meta = model_metadata_for_namespace(Some("mistral"), "codestral-latest")
            .expect("mistral qualified entry");
        assert_eq!(meta.id, "codestral-latest");
        assert!(meta.context_window.is_some());
        let warmed = model_metadata_for_namespaces(&[(Some("mistral"), "codestral-latest")]);
        assert_eq!(warmed.len(), 1);
        assert_eq!(warmed[0].id, "codestral-latest");
    }

    #[test]
    fn batch_providers_warms_cache_in_order() {
        let pairs: &[(Option<&str>, &str)] = &[
            (Some("builtin-mistral"), "codestral-latest"),
            (None, "gpt-4o"),
            (None, "no-such-model-xyz"),
        ];
        let warmed = model_metadata_for_providers(pairs);
        assert_eq!(warmed.len(), 2);
        assert_eq!(warmed[0].id, "codestral-latest");
        assert_eq!(warmed[1].id, "gpt-4o");
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

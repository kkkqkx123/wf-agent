//! Built-in provider connection templates (host-owned).
//!
//! These are static connection baselines for the well-known providers:
//! base URL, wire format, auth type and discovery mode. They carry no
//! model rows and no prices — model metadata comes from the host pricing
//! table (`wf-pricing`, derived from LiteLLM), live discovery fills the
//! rest. Registration reuses the existing `ProviderDefinitionRegistry`;
//! file-layer definitions registered afterwards win on id collision.

use llm_types::llm::{LlmFormat, LlmProviderDefinition, ProviderCompat};

use llm_codec::error::LlmResult;
use llm_config::provider::ProviderDefinitionRegistry;

/// Built-in provider definition ids. Stable strings hosts can rely on.
pub const BUILTIN_OPENAI_ID: &str = "builtin-openai";
pub const BUILTIN_ANTHROPIC_ID: &str = "builtin-anthropic";
pub const BUILTIN_GEMINI_ID: &str = "builtin-gemini";
pub const BUILTIN_OLLAMA_ID: &str = "builtin-ollama";
pub const BUILTIN_OPENROUTER_ID: &str = "builtin-openrouter";

/// Static connection templates for the well-known providers.
///
/// Returned in a fixed order (OpenAI, Anthropic, Gemini, Ollama,
/// OpenRouter). Callers register them through the existing
/// `ProviderDefinitionRegistry`; there is no new registry.
pub fn builtin_provider_definitions() -> Vec<LlmProviderDefinition> {
    vec![
        LlmProviderDefinition {
            id: BUILTIN_OPENAI_ID.to_string(),
            name: Some("OpenAI".to_string()),
            description: Some("OpenAI Chat Completions API".to_string()),
            base_url: Some("https://api.openai.com/v1".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(llm_types::llm::ModelDiscovery::ModelsEndpoint {
                path: None,
                json_path: None,
            }),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_ANTHROPIC_ID.to_string(),
            name: Some("Anthropic".to_string()),
            description: Some("Anthropic Messages API".to_string()),
            base_url: Some("https://api.anthropic.com/v1".to_string()),
            auth_type: Some("x-api-key".to_string()),
            default_headers: None,
            format: LlmFormat::Anthropic,
            model_discovery: Some(llm_types::llm::ModelDiscovery::ModelsEndpoint {
                path: None,
                json_path: None,
            }),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_GEMINI_ID.to_string(),
            name: Some("Gemini".to_string()),
            description: Some("Google Gemini native API".to_string()),
            base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
            auth_type: Some("x-goog-api-key".to_string()),
            default_headers: None,
            format: LlmFormat::GeminiNative,
            model_discovery: Some(llm_types::llm::ModelDiscovery::Disabled),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_OLLAMA_ID.to_string(),
            name: Some("Ollama".to_string()),
            description: Some("Local Ollama OpenAI-compatible gateway".to_string()),
            base_url: Some("http://localhost:11434/v1".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(llm_types::llm::ModelDiscovery::ModelsEndpoint {
                path: None,
                json_path: None,
            }),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            // Explicit legacy field for the local gateway: duplicates what
            // the narrow auto-detection would guess, so the baseline holds
            // even if the detection signal ever narrows further. Explicit
            // config always wins over detection.
            compat: Some(ProviderCompat {
                max_tokens_field: Some(llm_types::llm::generation::LlmMaxTokenField::MaxTokens),
                ..Default::default()
            }),
            identity: None,
            transport: None,
            env: None,
        },
        LlmProviderDefinition {
            id: BUILTIN_OPENROUTER_ID.to_string(),
            name: Some("OpenRouter".to_string()),
            description: Some("OpenRouter OpenAI-compatible gateway".to_string()),
            base_url: Some("https://openrouter.ai/api/v1".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: Some(llm_types::llm::ModelDiscovery::ModelsEndpoint {
                path: None,
                json_path: None,
            }),
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        },
    ]
}

/// All built-in provider ids in registration order.
pub fn builtin_provider_ids() -> Vec<String> {
    builtin_provider_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect()
}

/// Register every built-in provider definition on a registry.
///
/// File-layer definitions registered afterwards win on id collision
/// because registration replaces by id.
pub fn register_builtin_providers(registry: &ProviderDefinitionRegistry) -> LlmResult<()> {
    for definition in builtin_provider_definitions() {
        registry.register(definition)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_providers_cover_all_wire_formats() {
        let definitions = builtin_provider_definitions();
        assert!(definitions.len() >= 4);
        let ids = builtin_provider_ids();
        assert!(ids.contains(&BUILTIN_OPENAI_ID.to_string()));
        assert!(ids.contains(&BUILTIN_ANTHROPIC_ID.to_string()));
        assert!(ids.contains(&BUILTIN_GEMINI_ID.to_string()));
        assert!(ids.contains(&BUILTIN_OLLAMA_ID.to_string()));
    }

    #[test]
    fn local_builtin_keeps_legacy_max_tokens_field() {
        let ollama = builtin_provider_definitions()
            .into_iter()
            .find(|d| d.id == BUILTIN_OLLAMA_ID)
            .expect("ollama");
        assert_eq!(
            ollama.compat.and_then(|c| c.max_tokens_field),
            Some(llm_types::llm::generation::LlmMaxTokenField::MaxTokens)
        );
    }

    #[test]
    fn register_builtin_providers_fills_registry() {
        let registry = ProviderDefinitionRegistry::new();
        register_builtin_providers(&registry).expect("register");
        assert_eq!(registry.size(), builtin_provider_definitions().len());
        // Re-registering is idempotent (replace by id).
        register_builtin_providers(&registry).expect("re-register");
        assert_eq!(registry.size(), builtin_provider_definitions().len());
    }
}

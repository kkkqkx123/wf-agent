//! Model metadata derived from the embedded LiteLLM table.
//!
//! Same source as pricing (`litellm_public.json`), same refresh cadence:
//! there is exactly one model catalog in this crate. Pricing resolution
//! stays in `static_table` (cost path); this module answers the
//! complementary questions — context window, image input, reasoning — as
//! [`ModelMetadata`] rows built on demand. No hardcoded model lists live
//! anywhere; entries unknown to LiteLLM yield `None` and callers keep
//! their explicit config.

use std::collections::HashMap;

use serde::Deserialize;

use llm_types::llm::{
    generation::ThinkingLevel, InputModality, LlmFormat, ModelMetadata, ModelType,
};

/// Subset of a LiteLLM entry this resolver reads. Unknown fields are
/// ignored so upstream schema additions never break the build.
#[derive(Debug, Clone, Default, Deserialize)]
struct LiteLlmEntry {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    litellm_provider: String,
    #[serde(default)]
    max_input_tokens: u64,
    #[serde(default)]
    max_output_tokens: u64,
    #[serde(default)]
    supports_vision: bool,
    #[serde(default)]
    supports_reasoning: bool,
    #[serde(default)]
    supported_modalities: Vec<String>,
    #[serde(default)]
    reasoning_effort_levels: Vec<String>,
}

/// Metadata resolver backed by the embedded LiteLLM public table.
pub struct ModelMetadataResolver {
    table: HashMap<String, LiteLlmEntry>,
}

impl ModelMetadataResolver {
    /// Build from the embedded `litellm_public.json`.
    pub fn from_embedded() -> Self {
        let json = include_str!(concat!(env!("OUT_DIR"), "/litellm_public.json"));
        Self::from_json(json)
    }

    fn from_json(json: &str) -> Self {
        // Two-stage parse: `sample_spec` documents the schema with string
        // placeholders where numbers belong, so it can never deserialize
        // into `LiteLlmEntry` and is removed before typed parsing. A real
        // entry that breaks the schema then still fails loudly.
        let mut raw: HashMap<String, serde_json::Value> =
            serde_json::from_str(json).expect("embedded litellm_public.json must be valid");
        raw.remove("sample_spec");
        let mut table = HashMap::new();
        for (model, entry) in raw {
            let entry: LiteLlmEntry = serde_json::from_value(entry)
                .expect("litellm entry must match the metadata schema");
            // Keep only conversational metadata; other modes are never
            // exposed through `ModelMetadata` and would only waste resident
            // memory. `responses` shares the chat shape (prompt in,
            // message out), so vendors serving it still get window hints.
            if !matches!(
                entry.mode.as_str(),
                "chat" | "responses" | "image_generation"
            ) {
                continue;
            }
            table.insert(model, entry);
        }
        Self { table }
    }

    /// Metadata for one model name, or `None` when LiteLLM does not know
    /// it or it is not a conversational model. Cost is intentionally left
    /// empty: the pricing resolver remains the single cost path.
    pub fn metadata_for(&self, model: &str) -> Option<ModelMetadata> {
        self.metadata_for_provider(None, model)
    }

    /// Metadata for one model with an optional LiteLLM provider namespace.
    ///
    /// Gateway-hosted models often exist only under a provider-qualified
    /// key (such as `mistral/codestral-latest`) with no canonical short
    /// name, so the qualified key is tried first when a namespace is given
    /// (or when the model itself is already qualified), falling back to the
    /// canonical short name. The returned id echoes the requested model
    /// name rather than the table key so callers can match it directly.
    pub fn metadata_for_provider(
        &self,
        litellm_provider: Option<&str>,
        model: &str,
    ) -> Option<ModelMetadata> {
        if model.contains('/') {
            if let Some(meta) = build_metadata(model, self.table.get(model)?) {
                return Some(meta);
            }
        }
        if let Some(namespace) = litellm_provider {
            if !model.contains('/') {
                let qualified = format!("{namespace}/{model}");
                if let Some(entry) = self.table.get(&qualified) {
                    if let Some(meta) = build_metadata(model, entry) {
                        return Some(meta);
                    }
                }
            }
        }
        let entry = self.table.get(model)?;
        build_metadata(model, entry)
    }
}

fn build_metadata(model: &str, entry: &LiteLlmEntry) -> Option<ModelMetadata> {
    let model_type = match entry.mode.as_str() {
        "chat" | "responses" => ModelType::Chat,
        "image_generation" => ModelType::Image,
        _ => return None,
    };
    let image = entry.supports_vision
        || entry
            .supported_modalities
            .iter()
            .any(|m| m.eq_ignore_ascii_case("image"));
    let provider = entry.litellm_provider.clone();
    Some(ModelMetadata {
        id: model.to_string(),
        provider_id: if provider.is_empty() {
            "litellm".to_string()
        } else {
            provider.clone()
        },
        model_type,
        format: Some(format_for_provider(&provider)),
        context_window: nonzero_u32(entry.max_input_tokens),
        max_output_tokens: nonzero_u32(entry.max_output_tokens),
        input_modalities: if image {
            vec![InputModality::Text, InputModality::Image]
        } else {
            vec![InputModality::Text]
        },
        reasoning: entry.supports_reasoning,
        thinking_levels: entry
            .reasoning_effort_levels
            .iter()
            .filter_map(|level| parse_thinking_level(level))
            .collect(),
        cost: None,
    })
}

fn parse_thinking_level(value: &str) -> Option<ThinkingLevel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some(ThinkingLevel::None),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::Xhigh),
        "max" => Some(ThinkingLevel::Max),
        _ => None,
    }
}

fn nonzero_u32(value: u64) -> Option<u32> {
    u32::try_from(value).ok().filter(|v| *v > 0)
}

/// Wire format guess from the LiteLLM provider namespace. Anthropic and
/// Gemini families keep their native protocols; every other provider is
/// served through the OpenAI-compatible default.
fn format_for_provider(litellm_provider: &str) -> LlmFormat {
    let provider = litellm_provider.to_ascii_lowercase();
    if provider.contains("anthropic") {
        LlmFormat::Anthropic
    } else if provider.contains("gemini") || provider.contains("vertex") {
        LlmFormat::GeminiNative
    } else {
        LlmFormat::OpenaiChat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> ModelMetadataResolver {
        ModelMetadataResolver::from_embedded()
    }

    #[test]
    fn openai_chat_model_maps_window_and_vision() {
        let meta = resolver().metadata_for("gpt-4o").expect("gpt-4o");
        assert_eq!(meta.context_window, Some(128_000));
        assert_eq!(meta.format, Some(LlmFormat::OpenaiChat));
        assert!(meta.supports_image());
        assert_eq!(meta.model_type, ModelType::Chat);
        // Cost stays on the pricing path, never duplicated here.
        assert!(meta.cost.is_none());
    }

    #[test]
    fn anthropic_model_maps_reasoning_and_format() {
        let meta = resolver()
            .metadata_for("claude-sonnet-4-5")
            .expect("claude-sonnet-4-5");
        assert_eq!(meta.format, Some(LlmFormat::Anthropic));
        assert!(meta.reasoning);
        assert_eq!(meta.context_window, Some(200_000));
    }

    #[test]
    fn gemini_model_maps_native_format_and_large_window() {
        let meta = resolver()
            .metadata_for("gemini-2.5-flash")
            .expect("gemini-2.5-flash");
        assert_eq!(meta.format, Some(LlmFormat::GeminiNative));
        assert_eq!(meta.context_window, Some(1_048_576));
    }

    #[test]
    fn unknown_and_non_chat_models_yield_none() {
        let resolver = resolver();
        assert!(resolver.metadata_for("no-such-model-xyz").is_none());
        assert!(resolver.metadata_for("sample_spec").is_none());
        // Embedding models are not chat/image metadata.
        assert!(resolver.metadata_for("text-embedding-3-small").is_none());
    }

    #[test]
    fn responses_mode_maps_to_chat() {
        let resolver = resolver();
        let meta = resolver
            .metadata_for("azure/codex-mini")
            .expect("azure/codex-mini");
        assert_eq!(meta.model_type, ModelType::Chat);
        assert_eq!(meta.context_window, Some(200_000));
    }

    #[test]
    fn provider_qualified_lookup_falls_back_to_canonical() {
        let resolver = resolver();
        // Gateway-hosted models exist only under the qualified key.
        assert!(resolver.metadata_for("codestral-latest").is_none());
        let meta = resolver
            .metadata_for_provider(Some("mistral"), "codestral-latest")
            .expect("mistral/codestral-latest");
        assert_eq!(meta.id, "codestral-latest");
        assert!(meta.context_window.is_some());
        // Already-qualified names resolve directly.
        let direct = resolver
            .metadata_for("mistral/codestral-latest")
            .expect("direct qualified");
        assert_eq!(direct.context_window, meta.context_window);
        // Canonical short names still resolve without a namespace.
        assert!(resolver.metadata_for("gpt-4o").is_some());
        assert!(resolver
            .metadata_for_provider(Some("openai"), "gpt-4o")
            .is_some());
    }
}

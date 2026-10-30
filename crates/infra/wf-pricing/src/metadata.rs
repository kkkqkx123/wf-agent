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
            table.insert(model, entry);
        }
        Self { table }
    }

    /// Metadata for one model name, or `None` when LiteLLM does not know
    /// it or it is not a chat/image model. Cost is intentionally left
    /// empty: the pricing resolver remains the single cost path.
    pub fn metadata_for(&self, model: &str) -> Option<ModelMetadata> {
        let entry = self.table.get(model)?;
        let model_type = match entry.mode.as_str() {
            "chat" => ModelType::Chat,
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
}

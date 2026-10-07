//! Chat configuration types.
//!
//! Ported from code-context-engine `cce-llm` config: sampling parameters plus
//! optional penalties, stop sequences, seed, and response format.

use serde::{Deserialize, Serialize};

/// Chat completion configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Model name (e.g. `gpt-4o-mini`).
    pub model: String,
    /// Maximum tokens to generate.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Sampling temperature.
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Nucleus sampling parameter.
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    /// Frequency penalty, omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Presence penalty, omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Stop sequences, omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    /// Seed for deterministic results, omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// Response format (e.g. JSON mode), omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
}

/// Response format configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    /// Format type (e.g. `json_object`, `text`).
    #[serde(rename = "type")]
    pub format_type: String,
}

impl ChatConfig {
    /// Creates a config for the given model with default sampling parameters.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Self::default()
        }
    }
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            frequency_penalty: None,
            presence_penalty: None,
            stop_sequences: Vec::new(),
            seed: None,
            response_format: None,
        }
    }
}

fn default_max_tokens() -> u32 {
    1024
}

fn default_temperature() -> f32 {
    0.3
}

fn default_top_p() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_source_values() {
        let config = ChatConfig::new("gpt-4o-mini");
        assert_eq!(config.max_tokens, 1024);
        assert!((config.temperature - 0.3).abs() < f32::EPSILON);
        assert!((config.top_p - 1.0).abs() < f32::EPSILON);
        assert!(config.frequency_penalty.is_none());
        assert!(config.stop_sequences.is_empty());
    }

    #[test]
    fn optional_fields_skip_serializing_when_empty() {
        let value = serde_json::to_value(ChatConfig::new("m")).expect("serializable");
        let object = value.as_object().expect("object");
        for key in [
            "frequency_penalty",
            "presence_penalty",
            "stop_sequences",
            "seed",
            "response_format",
        ] {
            assert!(!object.contains_key(key), "unexpected key {key}");
        }
    }
}

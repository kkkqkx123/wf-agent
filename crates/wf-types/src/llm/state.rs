use serde::{Deserialize, Serialize};

/// LLM provider. The five built-in providers are constructed by the static
/// factories; `Custom` addresses a formatter registered at runtime through
/// the formatter registry (plugin extension point).
///
/// Serde is implemented via the canonical string form: built-ins use their
/// `SCREAMING_SNAKE_CASE` name, any other string maps to `Custom(name)`.
#[derive(Debug, Clone, PartialEq)]
pub enum LlmProvider {
    OpenaiChat,
    OpenaiResponse,
    Anthropic,
    GeminiNative,
    GeminiOpenai,
    /// Provider whose formatter is resolved through the runtime registry.
    Custom(String),
}

impl LlmProvider {
    /// Canonical string form: the `SCREAMING_SNAKE_CASE` name of the built-in
    /// providers, or the raw registered name for `Custom`.
    pub fn as_str(&self) -> &str {
        match self {
            LlmProvider::OpenaiChat => "OPENAI_CHAT",
            LlmProvider::OpenaiResponse => "OPENAI_RESPONSE",
            LlmProvider::Anthropic => "ANTHROPIC",
            LlmProvider::GeminiNative => "GEMINI_NATIVE",
            LlmProvider::GeminiOpenai => "GEMINI_OPENAI",
            LlmProvider::Custom(name) => name,
        }
    }
}

impl std::str::FromStr for LlmProvider {
    type Err = core::convert::Infallible;

    /// Parse a provider from its canonical string form. Every string is
    /// valid: built-ins map to their variant, anything else is `Custom`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "OPENAI_CHAT" => LlmProvider::OpenaiChat,
            "OPENAI_RESPONSE" => LlmProvider::OpenaiResponse,
            "ANTHROPIC" => LlmProvider::Anthropic,
            "GEMINI_NATIVE" => LlmProvider::GeminiNative,
            "GEMINI_OPENAI" => LlmProvider::GeminiOpenai,
            other => LlmProvider::Custom(other.to_string()),
        })
    }
}

impl Serialize for LlmProvider {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LlmProvider {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.parse::<LlmProvider>() {
            Ok(provider) => Ok(provider),
            Err(infallible) => match infallible {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_roundtrip() {
        for provider in [
            LlmProvider::OpenaiChat,
            LlmProvider::OpenaiResponse,
            LlmProvider::Anthropic,
            LlmProvider::GeminiNative,
            LlmProvider::GeminiOpenai,
        ] {
            let json = serde_json::to_string(&provider).unwrap();
            let back: LlmProvider = serde_json::from_str(&json).unwrap();
            assert_eq!(back, provider);
        }
        assert_eq!(
            serde_json::to_string(&LlmProvider::OpenaiChat).unwrap(),
            "\"OPENAI_CHAT\""
        );
        assert_eq!(
            serde_json::from_str::<LlmProvider>("\"ANTHROPIC\"").unwrap(),
            LlmProvider::Anthropic
        );
    }

    #[test]
    fn custom_roundtrip() {
        let provider = LlmProvider::Custom("my_provider".to_string());
        let json = serde_json::to_string(&provider).unwrap();
        assert_eq!(json, "\"my_provider\"");
        let back: LlmProvider = serde_json::from_str(&json).unwrap();
        assert_eq!(back, provider);

        let parsed: LlmProvider =
            serde_json::from_str("\"MY_GATEWAY_PROVIDER\"").unwrap();
        assert_eq!(
            parsed,
            LlmProvider::Custom("MY_GATEWAY_PROVIDER".to_string())
        );
    }

    #[test]
    fn as_str_matches_serde() {
        for provider in [
            LlmProvider::OpenaiChat,
            LlmProvider::OpenaiResponse,
            LlmProvider::Anthropic,
            LlmProvider::GeminiNative,
            LlmProvider::GeminiOpenai,
        ] {
            let from_name = provider.as_str().parse::<LlmProvider>().unwrap();
            assert_eq!(from_name, provider);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmProviderConfig {
    pub id: String,
    pub name: String,
    pub api_type: LlmProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

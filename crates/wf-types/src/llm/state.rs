use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LlmProvider {
    OpenaiChat,
    OpenaiResponse,
    Anthropic,
    GeminiNative,
    GeminiOpenai,
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

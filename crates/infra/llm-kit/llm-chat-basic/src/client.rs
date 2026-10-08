//! Basic OpenAI-compatible chat HTTP client.
//!
//! Sends `POST {base_url}/chat/completions` and reads the first choice plus
//! token usage, mirroring the code-context-engine chat handler. Message
//! conversion flattens [`llm_types::Message`] into the OpenAI role/content
//! shape; `Tool` messages use the `"tool"` role.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use llm_types::{Message, MessageContent, MessageContentValue, MessageRole};

use crate::config::ChatConfig;
use crate::error::{ChatError, Result};
use crate::request_builder::RequestBuilder;

/// Chat completion result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResult {
    /// Generated message content.
    pub content: String,
    /// Prompt tokens reported by the provider.
    pub prompt_tokens: u64,
    /// Completion tokens derived as `total - prompt`.
    pub completion_tokens: u64,
    /// Total tokens reported by the provider.
    pub total_tokens: u64,
}

/// Port for basic chat completion capability.
#[async_trait]
pub trait LlmChat: Send + Sync {
    /// Sends a non-streaming chat request.
    async fn chat(&self, messages: &[Message], config: &ChatConfig) -> Result<ChatResult>;

    /// Sends a streaming chat request, returning the SSE event stream.
    #[cfg(feature = "streaming")]
    async fn chat_stream(
        &self,
        messages: &[Message],
        config: &ChatConfig,
    ) -> Result<crate::stream::ChatEventStream>;
}

/// Client for OpenAI-compatible `/chat/completions` endpoints.
#[derive(Debug, Clone)]
pub struct BasicChatClient {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl BasicChatClient {
    /// Creates a client for a base URL such as `https://api.openai.com/v1`.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        timeout_secs: u64,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs.max(1)))
            .build()
            .map_err(|err| ChatError::Transport(err.to_string()))?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            http,
        })
    }

    /// Returns the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Builds the request body for a message list and config.
    pub fn build_body(&self, messages: &[Message], config: &ChatConfig) -> Result<Value> {
        if messages.is_empty() {
            return Err(ChatError::InvalidRequest(
                "messages must not be empty".into(),
            ));
        }
        let api_messages: Vec<Value> = messages
            .iter()
            .map(|message| {
                json!({
                    "role": role_name(&message.role),
                    "content": flatten_content(&message.content),
                })
            })
            .collect();

        let mut builder = RequestBuilder::new(&config.model)
            .with_messages(api_messages)
            .with_max_tokens(config.max_tokens)
            .with_temperature(config.temperature)
            .with_top_p(config.top_p)
            .with_frequency_penalty(config.frequency_penalty)
            .with_presence_penalty(config.presence_penalty)
            .with_stop_sequences(&config.stop_sequences)
            .with_seed(config.seed);
        if let Some(format) = &config.response_format {
            builder = builder.with_response_format(&format.format_type);
        }
        Ok(builder.build())
    }

    /// Sends the body and decodes the chat response.
    async fn post_chat(&self, body: &Value) -> Result<ChatResult> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut outgoing = self.http.post(&url).json(body);
        if let Some(api_key) = &self.api_key {
            outgoing = outgoing.bearer_auth(api_key);
        }
        let response = outgoing.send().await?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ChatError::Provider {
                status: status.as_u16(),
                message: text,
            });
        }
        let decoded: Value = response
            .json()
            .await
            .map_err(|err| ChatError::Decode(err.to_string()))?;
        parse_chat_response(&decoded)
    }
}

/// Maps a shared role onto the OpenAI role string.
fn role_name(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

/// Flattens message content into the plain-text body the endpoint expects.
fn flatten_content(content: &MessageContentValue) -> String {
    match content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                MessageContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

/// Decodes a `/chat/completions` response into a [`ChatResult`].
pub fn parse_chat_response(body: &Value) -> Result<ChatResult> {
    let content = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| ChatError::Decode("chat response contains no choices".into()))?;

    let prompt_tokens = body
        .pointer("/usage/prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = body
        .pointer("/usage/total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    Ok(ChatResult {
        content: content.to_string(),
        prompt_tokens,
        completion_tokens: total_tokens.saturating_sub(prompt_tokens),
        total_tokens,
    })
}

#[async_trait]
impl LlmChat for BasicChatClient {
    async fn chat(&self, messages: &[Message], config: &ChatConfig) -> Result<ChatResult> {
        let body = self.build_body(messages, config)?;
        self.post_chat(&body).await
    }

    #[cfg(feature = "streaming")]
    async fn chat_stream(
        &self,
        messages: &[Message],
        config: &ChatConfig,
    ) -> Result<crate::stream::ChatEventStream> {
        let mut body = self.build_body(messages, config)?;
        if let Some(object) = body.as_object_mut() {
            object.insert("stream".into(), Value::Bool(true));
        }
        let url = format!("{}/chat/completions", self.base_url);
        let mut outgoing = self.http.post(&url).json(&body);
        if let Some(api_key) = &self.api_key {
            outgoing = outgoing.bearer_auth(api_key);
        }
        let response = outgoing.send().await?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ChatError::Provider {
                status: status.as_u16(),
                message: text,
            });
        }
        Ok(crate::stream::ChatEventStream::new(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_body_maps_roles_and_config() {
        let client =
            BasicChatClient::new("https://api.example.com/v1/", None, 30).expect("client builds");
        assert_eq!(client.base_url(), "https://api.example.com/v1");

        let messages = vec![
            Message::text(MessageRole::System, "Be helpful"),
            Message::text(MessageRole::User, "Hello"),
        ];
        let config = ChatConfig::new("gpt-4o-mini");
        let body = client.build_body(&messages, &config).expect("body");

        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "Hello");
        assert_eq!(body["max_tokens"], 1024);
    }

    #[test]
    fn build_body_rejects_empty_messages() {
        let client =
            BasicChatClient::new("https://api.example.com/v1", None, 30).expect("client builds");
        let result = client.build_body(&[], &ChatConfig::new("m"));
        assert!(matches!(result, Err(ChatError::InvalidRequest(_))));
    }

    #[test]
    fn parse_response_reads_first_choice_and_usage() {
        let body = json!({
            "choices": [{"message": {"content": "Hi there"}}],
            "usage": {"prompt_tokens": 10, "total_tokens": 25},
        });
        let result = parse_chat_response(&body).expect("parsed");
        assert_eq!(result.content, "Hi there");
        assert_eq!(result.prompt_tokens, 10);
        assert_eq!(result.completion_tokens, 15);
        assert_eq!(result.total_tokens, 25);
    }

    #[test]
    fn parse_response_rejects_missing_choices() {
        let body = json!({"choices": []});
        assert!(parse_chat_response(&body).is_err());
    }
}

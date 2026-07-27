use async_trait::async_trait;
use reqwest::Client as ReqwestClient;
use std::sync::Arc;
use std::time::Instant;
use wf_types::llm::{LlmRequest, LlmResult as LlmResponseType, LlmProfile};
use wf_types::message::MessageContentValue;
use crate::error::{LlmError, LlmResult};
use crate::formatters::LlmFormatter;
use crate::message_stream::MessageStream;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate(&self, request: &LlmRequest) -> LlmResult<LlmResponseType>;
    async fn generate_stream(&self, request: &LlmRequest) -> LlmResult<Box<dyn MessageStream>>;
    async fn count_tokens(&self, request: &LlmRequest) -> LlmResult<u32>;
}

pub struct LlmClientImpl {
    client: ReqwestClient,
    formatter: Arc<dyn LlmFormatter>,
    profile: LlmProfile,
}

impl LlmClientImpl {
    pub fn new(client: ReqwestClient, formatter: Arc<dyn LlmFormatter>, profile: LlmProfile) -> Self {
        Self { client, formatter, profile }
    }

    pub fn profile(&self) -> &LlmProfile {
        &self.profile
    }
}

#[async_trait]
impl LlmClient for LlmClientImpl {
    async fn generate(&self, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let start = Instant::now();

        let http_request = self.formatter.build_request(request, &self.profile)?;
        let response = self.client.execute(http_request).await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::ProviderError(format!("HTTP {}: {}", status, body)));
        }

        let body = response.text().await?;
        let mut result = self.formatter.parse_response(&body)?;

        result.duration = start.elapsed().as_millis() as i64;

        Ok(result)
    }

    async fn generate_stream(&self, request: &LlmRequest) -> LlmResult<Box<dyn MessageStream>> {
        let mut stream_request = request.clone();
        stream_request.stream = Some(true);

        let http_request = self.formatter.build_request(&stream_request, &self.profile)?;
        let response = self.client.execute(http_request).await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::ProviderError(format!("HTTP {}: {}", status, body)));
        }

        let stream = eventsource_stream::EventStream::new(response.bytes_stream());

        Ok(Box::new(crate::message_stream::SseMessageStream::new(stream)))
    }

    async fn count_tokens(&self, request: &LlmRequest) -> LlmResult<u32> {
        let mut total = 0u32;

        for msg in &request.messages {
            match &msg.content {
                MessageContentValue::Text(text) => {
                    total += estimate_tokens(text);
                }
                MessageContentValue::Rich(contents) => {
                    for content in contents {
                        if let wf_types::message::MessageContent::Text { text } = content {
                            total += estimate_tokens(text);
                        }
                    }
                }
            }
        }

        if let Some(tools) = &request.tools {
            for tool in tools {
                total += estimate_tokens(&tool.name);
                total += estimate_tokens(&tool.description);
            }
        }

        Ok(total)
    }
}

fn estimate_tokens(text: &str) -> u32 {
    let chars = text.chars().count();
    (chars as f64 / 4.0).ceil() as u32
}

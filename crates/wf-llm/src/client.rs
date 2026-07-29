use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client as ReqwestClient;
use tokio_util::sync::CancellationToken;
use wf_types::llm::{LlmRequest, LlmResult as LlmResponseType, LlmProfile};
use wf_types::message::MessageContentValue;
use crate::error::{LlmError, LlmResult};
use crate::formatters::LlmFormatter;
use crate::message_stream::MessageStream;

pub trait LlmClient: Send + Sync {
    fn generate(&self, request: &LlmRequest) -> impl Future<Output = LlmResult<LlmResponseType>> + Send;
    fn generate_stream(&self, request: &LlmRequest) -> impl Future<Output = LlmResult<Box<dyn MessageStream>>> + Send;
    fn count_tokens(&self, request: &LlmRequest) -> impl Future<Output = LlmResult<u32>> + Send;
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

    fn build_timeout(&self) -> Duration {
        Duration::from_secs(self.profile.timeout.unwrap_or(60))
    }

    fn retry_delay(&self, attempt: u32) -> Duration {
        let base = self.profile.retry_delay.unwrap_or(1000);
        Duration::from_millis(base * 2u64.pow(attempt))
    }

    fn max_retries(&self) -> u32 {
        self.profile.max_retries.unwrap_or(3)
    }

    fn map_http_error(status: reqwest::StatusCode, body: &str, timeout_ms: u64) -> LlmError {
        if status.is_success() {
            return LlmError::InvalidResponse(format!("Unexpected success status with body: {}", body));
        }
        let msg = format!("HTTP {}: {}", status.as_u16(), body);
        match status.as_u16() {
            401 | 403 => LlmError::AuthError(msg),
            408 | 504 => LlmError::Timeout(timeout_ms),
            _ => LlmError::ProviderError(msg),
        }
    }
}

impl LlmClientImpl {
    async fn generate_inner(&self, request: &LlmRequest, cancel: Option<CancellationToken>) -> LlmResult<LlmResponseType> {
        let start = Instant::now();

        let http_request = self.formatter.build_request(request, &self.profile)?;

        let timeout_dur = self.build_timeout();
        let timeout_ms = timeout_dur.as_millis() as u64;

        let response = if let Some(ref cancel) = cancel {
            let req = self.client.execute(http_request);
            tokio::select! {
                result = req => result.map_err(LlmError::HttpError)?,
                _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                _ = tokio::time::sleep(timeout_dur) => {
                    return Err(LlmError::Timeout(timeout_ms));
                }
            }
        } else {
            tokio::time::timeout(timeout_dur, self.client.execute(http_request))
                .await
                .map_err(|_| LlmError::Timeout(timeout_ms))?
                .map_err(|e| {
                    if e.is_timeout() {
                        LlmError::Timeout(timeout_ms)
                    } else {
                        LlmError::HttpError(e)
                    }
                })?
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Self::map_http_error(status, &body, timeout_ms));
        }

        let body = response.text().await?;
        let mut result = self.formatter.parse_response(&body)?;

        result.duration = start.elapsed().as_millis() as i64;

        Ok(result)
    }

    async fn generate_stream_inner(&self, request: &LlmRequest, cancel: Option<CancellationToken>) -> LlmResult<Box<dyn MessageStream>> {
        let mut stream_request = request.clone();
        stream_request.stream = Some(true);

        let http_request = self.formatter.build_request(&stream_request, &self.profile)?;

        let timeout_dur = self.build_timeout();
        let timeout_ms = timeout_dur.as_millis() as u64;

        let response = if let Some(ref cancel) = cancel {
            let req = self.client.execute(http_request);
            tokio::select! {
                result = req => result.map_err(LlmError::HttpError)?,
                _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                _ = tokio::time::sleep(timeout_dur) => {
                    return Err(LlmError::Timeout(timeout_ms));
                }
            }
        } else {
            tokio::time::timeout(timeout_dur, self.client.execute(http_request))
                .await
                .map_err(|_| LlmError::Timeout(timeout_ms))?
                .map_err(|e| {
                    if e.is_timeout() {
                        LlmError::Timeout(timeout_ms)
                    } else {
                        LlmError::HttpError(e)
                    }
                })?
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Self::map_http_error(status, &body, timeout_ms));
        }

        let stream = eventsource_stream::EventStream::new(response.bytes_stream());

        Ok(Box::new(crate::message_stream::SseMessageStream::new(
            stream,
            self.formatter.clone(),
            cancel,
        )))
    }
}

impl LlmClient for LlmClientImpl {
    async fn generate(&self, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let max_retries = self.max_retries();
        let mut last_err = None;

        for attempt in 0..=max_retries {
            match self.generate_inner(request, None).await {
                Ok(result) => return Ok(result),
                Err(e) if e.is_retryable() && attempt < max_retries => {
                    last_err = Some(e);
                    tokio::time::sleep(self.retry_delay(attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or_else(|| LlmError::ConfigError("retry failed without error".to_string())))
    }

    async fn generate_stream(&self, request: &LlmRequest) -> LlmResult<Box<dyn MessageStream>> {
        let max_retries = self.max_retries();
        let mut last_err = None;

        for attempt in 0..=max_retries {
            match self.generate_stream_inner(request, None).await {
                Ok(stream) => return Ok(stream),
                Err(e) if e.is_retryable() && attempt < max_retries => {
                    last_err = Some(e);
                    tokio::time::sleep(self.retry_delay(attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or_else(|| LlmError::ConfigError("retry failed without error".to_string())))
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

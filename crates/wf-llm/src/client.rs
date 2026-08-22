use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::{LlmError, LlmResult};
use crate::formatters::LlmFormatter;
use crate::message_stream::MessageStream;
use reqwest::Client as ReqwestClient;
use tokio_util::sync::CancellationToken;
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType};

pub trait LlmClient: Send + Sync {
    fn generate(
        &self,
        request: &LlmRequest,
        cancel: Option<CancellationToken>,
    ) -> impl Future<Output = LlmResult<LlmResponseType>> + Send;
    fn generate_stream(
        &self,
        request: &LlmRequest,
        cancel: Option<CancellationToken>,
    ) -> impl Future<Output = LlmResult<Box<dyn MessageStream>>> + Send;
    fn count_tokens(
        &self,
        request: &LlmRequest,
        cancel: Option<CancellationToken>,
    ) -> impl Future<Output = LlmResult<wf_types::llm::TokenCountResult>> + Send;
}

pub struct LlmClientImpl {
    pub(crate) client: ReqwestClient,
    pub(crate) formatter: Arc<dyn LlmFormatter>,
    pub(crate) profile: LlmProfile,
}

impl LlmClientImpl {
    pub fn new(
        client: ReqwestClient,
        formatter: Arc<dyn LlmFormatter>,
        profile: LlmProfile,
    ) -> Self {
        Self {
            client,
            formatter,
            profile,
        }
    }

    pub fn profile(&self) -> &LlmProfile {
        &self.profile
    }

    pub(crate) fn build_timeout(&self) -> Duration {
        Duration::from_secs(self.profile.timeout.unwrap_or(60))
    }

    fn retry_delay(&self, attempt: u32) -> Duration {
        let base = self.profile.retry_delay.unwrap_or(1000);
        Duration::from_millis(base * 2u64.pow(attempt))
    }

    fn max_retries(&self) -> u32 {
        self.profile.max_retries.unwrap_or(3)
    }

    pub(crate) fn map_http_error(
        status: reqwest::StatusCode,
        body: &str,
        timeout_ms: u64,
    ) -> LlmError {
        if status.is_success() {
            return LlmError::InvalidResponse(format!(
                "Unexpected success status with body: {}",
                body
            ));
        }
        let msg = format!("HTTP {}: {}", status.as_u16(), body);
        // Safety-net classification: a context-window rejection of the actual
        // request is surfaced as ContextLengthExceeded so callers can force a
        // compression event when the local estimate undercounted.
        let provisional = match status.as_u16() {
            401 | 403 => LlmError::AuthError(msg),
            408 | 504 => LlmError::Timeout(timeout_ms),
            _ => LlmError::ProviderError(msg),
        };
        if provisional.is_context_length_exceeded() {
            return LlmError::ContextLengthExceeded(body.to_string());
        }
        provisional
    }
}

impl LlmClientImpl {
    async fn generate_inner(
        &self,
        request: &LlmRequest,
        cancel: Option<CancellationToken>,
    ) -> LlmResult<LlmResponseType> {
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
        let mut result = self.formatter.parse_response(&body, request)?;

        result.duration = start.elapsed().as_millis() as i64;

        Ok(result)
    }

    async fn generate_stream_inner(
        &self,
        request: &LlmRequest,
        cancel: Option<CancellationToken>,
    ) -> LlmResult<Box<dyn MessageStream>> {
        let mut stream_request = request.clone();
        stream_request.stream = Some(true);

        let http_request = self
            .formatter
            .build_request(&stream_request, &self.profile)?;

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
            request.dead_loop_detection.as_ref(),
        )))
    }
}

impl LlmClient for LlmClientImpl {
    async fn generate(
        &self,
        request: &LlmRequest,
        cancel: Option<CancellationToken>,
    ) -> LlmResult<LlmResponseType> {
        let max_retries = self.max_retries();
        let mut last_err = None;

        for attempt in 0..=max_retries {
            match self.generate_inner(request, cancel.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) if e.is_retryable() && attempt < max_retries => {
                    last_err = Some(e);
                    if let Some(ref cancel) = cancel {
                        tokio::select! {
                            _ = tokio::time::sleep(self.retry_delay(attempt)) => {}
                            _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                        }
                    } else {
                        tokio::time::sleep(self.retry_delay(attempt)).await;
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err
            .unwrap_or_else(|| LlmError::ConfigError("retry failed without error".to_string())))
    }

    async fn generate_stream(
        &self,
        request: &LlmRequest,
        cancel: Option<CancellationToken>,
    ) -> LlmResult<Box<dyn MessageStream>> {
        let max_retries = self.max_retries();
        let mut last_err = None;

        for attempt in 0..=max_retries {
            match self.generate_stream_inner(request, cancel.clone()).await {
                Ok(stream) => return Ok(stream),
                Err(e) if e.is_retryable() && attempt < max_retries => {
                    last_err = Some(e);
                    if let Some(ref cancel) = cancel {
                        tokio::select! {
                            _ = tokio::time::sleep(self.retry_delay(attempt)) => {}
                            _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                        }
                    } else {
                        tokio::time::sleep(self.retry_delay(attempt)).await;
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err
            .unwrap_or_else(|| LlmError::ConfigError("retry failed without error".to_string())))
    }

    async fn count_tokens(
        &self,
        request: &LlmRequest,
        cancel: Option<CancellationToken>,
    ) -> LlmResult<wf_types::llm::TokenCountResult> {
        crate::token_count::count_tokens_client(self, request, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::{LlmProfile, LlmProvider};

    fn profile(id: &str) -> LlmProfile {
        LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            provider: LlmProvider::OpenaiChat,
            model: "gpt-4o".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        }
    }

    #[test]
    fn map_http_error_classifies_auth_and_timeout() {
        assert!(matches!(
            LlmClientImpl::map_http_error(reqwest::StatusCode::UNAUTHORIZED, "denied", 5000),
            LlmError::AuthError(_)
        ));
        assert!(matches!(
            LlmClientImpl::map_http_error(reqwest::StatusCode::FORBIDDEN, "denied", 5000),
            LlmError::AuthError(_)
        ));
        assert!(matches!(
            LlmClientImpl::map_http_error(reqwest::StatusCode::REQUEST_TIMEOUT, "", 5000),
            LlmError::Timeout(5000)
        ));
        assert!(matches!(
            LlmClientImpl::map_http_error(reqwest::StatusCode::GATEWAY_TIMEOUT, "", 5000),
            LlmError::Timeout(5000)
        ));
    }

    #[test]
    fn map_http_error_classifies_provider_errors() {
        assert!(matches!(
            LlmClientImpl::map_http_error(reqwest::StatusCode::BAD_REQUEST, "bad", 5000),
            LlmError::ProviderError(_)
        ));
        assert!(matches!(
            LlmClientImpl::map_http_error(
                reqwest::StatusCode::TOO_MANY_REQUESTS,
                "slow down",
                5000
            ),
            LlmError::ProviderError(_)
        ));
        assert!(matches!(
            LlmClientImpl::map_http_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "boom", 5000),
            LlmError::ProviderError(_)
        ));
    }

    #[test]
    fn map_http_error_upgrades_context_length_messages() {
        let err = LlmClientImpl::map_http_error(
            reqwest::StatusCode::BAD_REQUEST,
            "This model's maximum context length is 200000 tokens",
            5000,
        );
        assert!(
            matches!(err, LlmError::ContextLengthExceeded(_)),
            "provider context-length rejection must be surfaced: {err:?}"
        );
    }

    #[test]
    fn map_http_error_flags_success_status() {
        let err = LlmClientImpl::map_http_error(reqwest::StatusCode::OK, "unexpected", 5000);
        assert!(matches!(err, LlmError::InvalidResponse(_)));
    }

    #[test]
    fn timeout_defaults_to_60_seconds() {
        let client = LlmClientImpl::new(
            reqwest::Client::new(),
            crate::formatters::create_formatter(&LlmProvider::OpenaiChat).unwrap(),
            profile("p1"),
        );
        assert_eq!(client.build_timeout(), Duration::from_secs(60));

        let mut p = profile("p1");
        p.timeout = Some(5);
        let client = LlmClientImpl::new(
            reqwest::Client::new(),
            crate::formatters::create_formatter(&LlmProvider::OpenaiChat).unwrap(),
            p,
        );
        assert_eq!(client.build_timeout(), Duration::from_secs(5));
    }

    #[test]
    fn retry_parameters_use_defaults_and_overrides() {
        let client = LlmClientImpl::new(
            reqwest::Client::new(),
            crate::formatters::create_formatter(&LlmProvider::OpenaiChat).unwrap(),
            profile("p1"),
        );
        assert_eq!(client.max_retries(), 3);
        assert_eq!(client.retry_delay(0), Duration::from_millis(1000));
        assert_eq!(
            client.retry_delay(2),
            Duration::from_millis(4000),
            "exponential backoff"
        );

        let mut p = profile("p1");
        p.max_retries = Some(5);
        p.retry_delay = Some(250);
        let client = LlmClientImpl::new(
            reqwest::Client::new(),
            crate::formatters::create_formatter(&LlmProvider::OpenaiChat).unwrap(),
            p,
        );
        assert_eq!(client.max_retries(), 5);
        assert_eq!(client.retry_delay(1), Duration::from_millis(500));
    }

    #[test]
    fn profile_accessor_returns_configured_profile() {
        let client = LlmClientImpl::new(
            reqwest::Client::new(),
            crate::formatters::create_formatter(&LlmProvider::OpenaiChat).unwrap(),
            profile("p1"),
        );
        assert_eq!(client.profile().id, "p1");
        assert_eq!(client.profile().model, "gpt-4o");
    }
}

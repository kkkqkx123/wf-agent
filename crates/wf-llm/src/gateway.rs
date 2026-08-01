use std::sync::Arc;

use dashmap::DashMap;
use wf_metrics::collectors::TokenMetricsCollector;
use wf_types::llm::{
    LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent, TokenUsageStats,
    ToolCallFormat, ToolCallProtocolViolationPolicy,
};

use crate::client::LlmClient;
use crate::client::LlmClientImpl;
use crate::error::{LlmError, LlmResult};
use crate::formatters::create_formatter;
use crate::message_stream::MessageStream;
use crate::profile_manager::ProfileManager;

/// Single facade for all LLM calls.
///
/// Responsibilities:
/// - resolve the profile for a mandatory `profile_id` (no fallback branch)
/// - merge profile defaults with request overrides in one place
/// - route to mock clients (test injection) or real clients
/// - record token usage metrics for both generate and stream paths
#[derive(Clone)]
pub struct LlmGateway {
    clients: Arc<DashMap<String, Arc<LlmClientImpl>>>,
    profiles: ProfileManager,
    #[cfg(feature = "mock")]
    mock_clients: Arc<DashMap<String, Arc<crate::mock::MockLlmClient>>>,
    token_metrics: Option<TokenMetricsCollector>,
}

impl LlmGateway {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            profiles: ProfileManager::new(),
            #[cfg(feature = "mock")]
            mock_clients: Arc::new(DashMap::new()),
            token_metrics: None,
        }
    }

    /// Attach an optional token usage collector (zero overhead when absent).
    pub fn with_token_metrics(mut self, token_metrics: TokenMetricsCollector) -> Self {
        self.token_metrics = Some(token_metrics);
        self
    }

    pub fn register_profile(&self, profile: LlmProfile) -> LlmResult<()> {
        self.profiles.register(profile)
    }

    /// Register a mock client under an arbitrary id (a real profile id can be
    /// reused, or a plain "mock" id). Mock hits take priority over profiles.
    #[cfg(feature = "mock")]
    pub fn register_mock(&self, id: impl Into<String>, client: Arc<crate::mock::MockLlmClient>) {
        self.mock_clients.insert(id.into(), client);
    }

    #[cfg(feature = "mock")]
    fn mock_client(&self, id: &str) -> Option<Arc<crate::mock::MockLlmClient>> {
        self.mock_clients.get(id).map(|c| c.clone())
    }

    /// Profile registry for assembly-time validation of profile references.
    pub fn profile_registry(&self) -> &ProfileManager {
        &self.profiles
    }

    pub fn has_profile(&self, id: &str) -> bool {
        self.profiles.get(id).is_some()
    }

    pub async fn generate(&self, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        #[cfg(feature = "mock")]
        if let Some(client) = self.mock_client(&request.profile_id) {
            return client.generate(request).await;
        }

        let profile = self.resolve_profile(&request.profile_id)?;
        let effective = self.merge_request(request, &profile)?;
        let client = self.get_or_create_client(&profile);
        let result = client.generate(&effective).await?;
        self.record_token_usage(&result, &profile);
        Ok(result)
    }

    pub async fn generate_stream(&self, request: &LlmRequest) -> LlmResult<Box<dyn MessageStream>> {
        #[cfg(feature = "mock")]
        if let Some(client) = self.mock_client(&request.profile_id) {
            return client.generate_stream(request).await;
        }

        let profile = self.resolve_profile(&request.profile_id)?;
        let effective = self.merge_request(request, &profile)?;
        let client = self.get_or_create_client(&profile);
        let stream = client.generate_stream(&effective).await?;
        Ok(Box::new(TokenRecordingStream::new(
            stream,
            self.token_metrics.clone(),
            profile.model.clone(),
        )))
    }

    pub async fn count_tokens(&self, request: &LlmRequest) -> LlmResult<u32> {
        #[cfg(feature = "mock")]
        if let Some(client) = self.mock_client(&request.profile_id) {
            return client.count_tokens(request).await;
        }

        let profile = self.resolve_profile(&request.profile_id)?;
        let effective = self.merge_request(request, &profile)?;
        let client = self.get_or_create_client(&profile);
        client.count_tokens(&effective).await
    }

    fn resolve_profile(&self, profile_id: &str) -> LlmResult<LlmProfile> {
        self.profiles
            .get(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(profile_id.to_string()))
    }

    fn get_or_create_client(&self, profile: &LlmProfile) -> Arc<LlmClientImpl> {
        let key = &profile.id;

        if let Some(client) = self.clients.get(key) {
            return client.clone();
        }

        let formatter = create_formatter(&profile.provider);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                profile.timeout.unwrap_or(60),
            ))
            .build()
            .unwrap_or_default();

        let client_impl = Arc::new(LlmClientImpl::new(client, formatter, profile.clone()));
        self.clients.insert(key.clone(), client_impl.clone());
        client_impl
    }

    /// Single-point merge of profile defaults with request overrides:
    /// - `parameters`: request wins per key, profile fills the rest
    /// - `tool_call_format`: request wins, otherwise the profile default
    /// - `locked_tool_call_format`: always wins when present, governed by the
    ///   violation policy (fail / warn / auto-convert / ignore)
    fn merge_request(&self, request: &LlmRequest, profile: &LlmProfile) -> LlmResult<LlmRequest> {
        let mut effective = request.clone();

        if effective.tool_call_format.is_none() {
            effective.tool_call_format = profile
                .tool_call_format
                .as_ref()
                .map(|config| config.format.clone());
        }

        if let Some(locked) = effective.locked_tool_call_format.clone() {
            if let Some(attempted) = effective.tool_call_format {
                // Compatible formats (e.g. both JSON-based) proceed silently;
                // a genuine protocol conflict is governed by the policy.
                if attempted != locked.format && !attempted.is_compatible_with(&locked.format) {
                    self.handle_protocol_violation(request, profile, &locked.format, attempted)?;
                }
            }
            effective.tool_call_format = Some(locked.format);
        }

        let merged = crate::formatter_helpers::merge_parameters(profile, &effective.parameters);
        effective.parameters = if merged.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(merged.into_iter().collect()))
        };

        Ok(effective)
    }

    fn handle_protocol_violation(
        &self,
        request: &LlmRequest,
        profile: &LlmProfile,
        locked: &ToolCallFormat,
        attempted: ToolCallFormat,
    ) -> LlmResult<()> {
        match request.violation_policy {
            Some(ToolCallProtocolViolationPolicy::Fail) => Err(LlmError::ConfigError(format!(
                "Tool call protocol conflict: locked \"{}\" but profile \"{}\" attempted \"{}\". Execution interrupted per fail policy.",
                locked, profile.id, attempted
            ))),
            Some(ToolCallProtocolViolationPolicy::Warn) => {
                tracing::warn!(
                    profile_id = %profile.id,
                    locked_format = %locked,
                    attempted_format = %attempted,
                    "Tool call protocol violation detected, using the locked format"
                );
                Ok(())
            }
            // Ignore / auto_convert: proceed with the locked format.
            _ => Ok(()),
        }
    }

    fn record_token_usage(&self, result: &LlmResponseType, profile: &LlmProfile) {
        let Some(collector) = self.token_metrics.as_ref() else {
            return;
        };
        let Some(usage) = result.usage.as_ref() else {
            return;
        };
        collector.record_token_usage(
            usage.prompt_tokens as u64,
            usage.completion_tokens as u64,
            usage.total_cost,
            Some(&profile.model),
        );
    }
}

impl Default for LlmGateway {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream wrapper that records token usage from mid-stream usage events
/// (OpenAI `include_usage` chunk, Anthropic `message_delta`) or, as a
/// fallback, from the final message once the stream is exhausted.
struct TokenRecordingStream {
    inner: Box<dyn MessageStream>,
    collector: Option<TokenMetricsCollector>,
    model: String,
    last_usage: Option<TokenUsageStats>,
    recorded: bool,
}

impl TokenRecordingStream {
    fn new(
        inner: Box<dyn MessageStream>,
        collector: Option<TokenMetricsCollector>,
        model: String,
    ) -> Self {
        Self {
            inner,
            collector,
            model,
            last_usage: None,
            recorded: false,
        }
    }

    fn record(&mut self, usage: &TokenUsageStats) {
        self.recorded = true;
        let Some(collector) = &self.collector else {
            return;
        };
        collector.record_token_usage(
            usage.prompt_tokens as u64,
            usage.completion_tokens as u64,
            usage.total_cost,
            Some(&self.model),
        );
    }
}

#[async_trait::async_trait]
impl MessageStream for TokenRecordingStream {
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>> {
        let event = self.inner.next().await;

        if let Some(Ok(MessageStreamEvent::Usage(usage))) = &event {
            self.last_usage = Some(usage.usage.clone());
            if !self.recorded {
                self.record(&usage.usage);
            }
        }

        if let Some(Ok(MessageStreamEvent::FinalMessage(msg))) = &event {
            self.last_usage = msg.usage.clone();
        }

        if event.is_none() && !self.recorded {
            if let Some(usage) = self.last_usage.clone() {
                self.record(&usage);
            }
        }

        event
    }
}

use std::sync::Arc;

use dashmap::DashMap;
use wf_metrics::collectors::TokenMetricsCollector;
use wf_types::llm::{
    LlmProfile, LlmRequest, LlmResult as LlmResponseType, MessageStreamEvent, StreamStats,
    TokenUsageStats, ToolCallFormat, ToolCallProtocolViolationPolicy,
    DEFAULT_TOOL_CALL_PROTOCOL_POLICY,
};

use crate::client::LlmClient;
use crate::client::LlmClientImpl;
use crate::error::{LlmError, LlmResult};
use crate::message_stream::MessageStream;
use crate::profile_manager::ProfileManager;
use crate::registry::FormatterRegistry;

/// Single facade for all LLM calls.
///
/// Responsibilities:
/// - resolve the profile for a mandatory `profile_id` (no fallback branch)
/// - merge profile defaults with request overrides in one place
/// - route to mock clients (test injection) or real clients
/// - resolve formatters through the registry (built-ins + runtime custom)
/// - record token usage metrics for both generate and stream paths
#[derive(Clone)]
pub struct LlmGateway {
    clients: Arc<DashMap<String, Arc<LlmClientImpl>>>,
    profiles: ProfileManager,
    formatters: FormatterRegistry,
    #[cfg(feature = "mock")]
    mock_clients: Arc<DashMap<String, Arc<crate::mock::MockLlmClient>>>,
    token_metrics: Option<TokenMetricsCollector>,
}

impl LlmGateway {
    pub fn new() -> Self {
        Self::new_with_formatter_registry(FormatterRegistry::new())
    }

    /// Create a gateway with a caller-provided formatter registry (custom
    /// providers must be registered on the registry before first use).
    pub fn new_with_formatter_registry(formatters: FormatterRegistry) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            profiles: ProfileManager::new(),
            formatters,
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

    /// The formatter registry: register custom providers here before the
    /// first request that uses them.
    pub fn formatter_registry(&self) -> &FormatterRegistry {
        &self.formatters
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
        let client = self.get_or_create_client(&profile)?;
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
        let client = self.get_or_create_client(&profile)?;
        let stream = client.generate_stream(&effective).await?;
        Ok(Box::new(TokenRecordingStream::new(
            stream,
            self.token_metrics.clone(),
            profile.model.clone(),
        )))
    }

    pub async fn count_tokens(
        &self,
        request: &LlmRequest,
    ) -> LlmResult<wf_types::llm::TokenCountResult> {
        #[cfg(feature = "mock")]
        if let Some(client) = self.mock_client(&request.profile_id) {
            return client.count_tokens(request).await;
        }

        let profile = self.resolve_profile(&request.profile_id)?;
        let effective = self.merge_request(request, &profile)?;
        let client = self.get_or_create_client(&profile)?;
        client.count_tokens(&effective).await
    }

    fn resolve_profile(&self, profile_id: &str) -> LlmResult<LlmProfile> {
        self.profiles
            .get(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(profile_id.to_string()))
    }

    fn get_or_create_client(&self, profile: &LlmProfile) -> LlmResult<Arc<LlmClientImpl>> {
        let key = format!("{}::{}", profile.id, profile.model);

        if let Some(client) = self.clients.get(key.as_str()) {
            return Ok(client.clone());
        }

        let formatter = self.formatters.get_by_provider(&profile.provider)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                profile.timeout.unwrap_or(60),
            ))
            .build()
            .unwrap_or_default();

        let client_impl = Arc::new(LlmClientImpl::new(client, formatter, profile.clone()));
        self.clients.insert(key, client_impl.clone());
        Ok(client_impl)
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
                    let policy = effective
                        .violation_policy
                        .clone()
                        .unwrap_or(DEFAULT_TOOL_CALL_PROTOCOL_POLICY);
                    self.handle_protocol_violation(
                        request,
                        profile,
                        &locked.format,
                        attempted,
                        policy.clone(),
                    )?;
                    if policy == ToolCallProtocolViolationPolicy::AutoConvert {
                        effective.protocol_auto_converted = Some(true);
                    }
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
        policy: ToolCallProtocolViolationPolicy,
    ) -> LlmResult<()> {
        match policy {
            ToolCallProtocolViolationPolicy::Fail => Err(LlmError::ConfigError(format!(
                "Tool call protocol conflict: locked \"{}\" but profile \"{}\" attempted \"{}\". Execution interrupted per fail policy.",
                locked, profile.id, attempted
            ))),
            ToolCallProtocolViolationPolicy::Warn => {
                tracing::warn!(
                    profile_id = %profile.id,
                    locked_format = %locked,
                    attempted_format = %attempted,
                    execution_id = ?request.execution_id,
                    "Tool call protocol violation detected, using the locked format"
                );
                Ok(())
            }
            ToolCallProtocolViolationPolicy::AutoConvert => {
                tracing::info!(
                    profile_id = %profile.id,
                    locked_format = %locked,
                    attempted_format = %attempted,
                    execution_id = ?request.execution_id,
                    "Auto-converting tool call protocol to locked format"
                );
                Ok(())
            }
            ToolCallProtocolViolationPolicy::Ignore => {
                // Silently use the locked protocol
                Ok(())
            }
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
/// fallback, from the final message once the stream is exhausted. It also
/// collects stream statistics (chunk count / first-chunk latency / stream
/// and total durations) and attaches them to the `FinalMessage` event.
struct TokenRecordingStream {
    inner: Box<dyn MessageStream>,
    collector: Option<TokenMetricsCollector>,
    model: String,
    last_usage: Option<TokenUsageStats>,
    recorded: bool,
    start_time: i64,
    first_chunk_time: Option<i64>,
    last_chunk_time: Option<i64>,
    chunk_count: u32,
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
            start_time: wf_common::time::now(),
            first_chunk_time: None,
            last_chunk_time: None,
            chunk_count: 0,
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

    fn build_stats(&self, end_time: i64) -> StreamStats {
        let first = self.first_chunk_time.unwrap_or(end_time);
        let last = self.last_chunk_time.unwrap_or(first);
        StreamStats {
            chunk_count: self.chunk_count,
            time_to_first_chunk: first - self.start_time,
            stream_duration: last.saturating_sub(first),
            total_duration: end_time.saturating_sub(self.start_time),
        }
    }
}

#[async_trait::async_trait]
impl MessageStream for TokenRecordingStream {
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>> {
        let mut event = self.inner.next().await;

        let now = wf_common::time::now();
        if event.is_some() {
            if self.first_chunk_time.is_none() {
                self.first_chunk_time = Some(now);
            }
            self.last_chunk_time = Some(now);
            self.chunk_count = self.chunk_count.saturating_add(1);
        }

        if let Some(Ok(MessageStreamEvent::Usage(usage))) = &event {
            self.last_usage = Some(usage.usage.clone());
            if !self.recorded {
                self.record(&usage.usage);
            }
        }

        if let Some(Ok(MessageStreamEvent::FinalMessage(msg))) = &mut event {
            self.last_usage = msg.usage.clone();
            if msg.stream_stats.is_none() {
                msg.stream_stats = Some(self.build_stats(now));
            }
        }

        if event.is_none() && !self.recorded {
            if let Some(usage) = self.last_usage.clone() {
                self.record(&usage);
            }
        }

        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LlmError;
    use crate::formatters::LlmFormatter;
    use crate::registry::FormatterRegistry;
    use wf_types::llm::{LlmProfile, LlmProvider, LlmRequest};
    use wf_types::tool::Tool;

    struct FakeStream {
        events: Vec<MessageStreamEvent>,
    }

    #[async_trait::async_trait]
    impl MessageStream for FakeStream {
        async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>> {
            if self.events.is_empty() {
                return None;
            }
            Some(Ok(self.events.remove(0)))
        }
    }

    fn text_event(text: &str) -> MessageStreamEvent {
        MessageStreamEvent::Text(wf_types::llm::MessageStreamText {
            text: text.to_string(),
            snapshot: text.to_string(),
        })
    }

    #[tokio::test]
    async fn final_message_carries_stream_stats() {
        let message = wf_types::message::Message {
            id: wf_types::Id::new(),
            role: wf_types::message::MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text("hello world".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let inner = FakeStream {
            events: vec![
                text_event("hello"),
                text_event(" world"),
                MessageStreamEvent::FinalMessage(wf_types::llm::MessageStreamFinal {
                    message,
                    usage: None,
                    stream_stats: None,
                }),
            ],
        };
        let mut stream = TokenRecordingStream::new(Box::new(inner), None, "test-model".to_string());

        let mut final_stats = None;
        while let Some(event) = stream.next().await {
            if let Ok(MessageStreamEvent::FinalMessage(msg)) = event {
                final_stats = msg.stream_stats;
            }
        }

        let stats = final_stats.expect("FinalMessage must carry stream_stats");
        assert!(stats.chunk_count >= 2, "chunk_count: {}", stats.chunk_count);
        assert!(
            stats.time_to_first_chunk >= 0,
            "time_to_first_chunk: {}",
            stats.time_to_first_chunk
        );
        assert!(stats.total_duration >= 0);
    }

    /// A formatter whose `build_request` fails with a distinctive error, used
    /// to prove the gateway resolved the *custom* formatter from the registry.
    struct ProbeFormatter;

    impl LlmFormatter for ProbeFormatter {
        fn build_request(
            &self,
            _request: &LlmRequest,
            _profile: &LlmProfile,
        ) -> LlmResult<reqwest::Request> {
            Err(LlmError::ConfigError("custom formatter engaged".to_string()))
        }

        fn parse_response(&self, _body: &str, _request: &LlmRequest) -> LlmResult<LlmResponseType> {
            Err(LlmError::ConfigError("custom formatter engaged".to_string()))
        }

        fn parse_stream_chunk(&self, _data: &str) -> LlmResult<Option<MessageStreamEvent>> {
            Ok(None)
        }

        fn convert_tools(&self, _tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
            Ok(Vec::new())
        }

        fn parse_tool_calls(&self, _result: &LlmResponseType) -> Vec<wf_types::message::LlmToolCall> {
            Vec::new()
        }
    }

    fn profile(id: &str, provider: LlmProvider) -> LlmProfile {
        LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            provider,
            model: "custom-model".to_string(),
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
        }
    }

    fn request(profile_id: &str) -> LlmRequest {
        LlmRequest {
            profile_id: profile_id.to_string(),
            messages: Vec::new(),
            parameters: None,
            tools: None,
            tool_call_format: None,
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        }
    }

    #[tokio::test]
    async fn custom_formatter_resolved_via_registry() {
        let registry = FormatterRegistry::new();
        registry
            .register("my_custom_provider", Arc::new(ProbeFormatter))
            .expect("custom registration must succeed");
        let gateway = LlmGateway::new_with_formatter_registry(registry);
        gateway
            .register_profile(profile(
                "p1",
                LlmProvider::Custom("my_custom_provider".to_string()),
            ))
            .unwrap();

        let err = gateway.generate(&request("p1")).await.unwrap_err();
        assert!(
            err.to_string().contains("custom formatter engaged"),
            "custom formatter must have been selected: {}",
            err
        );
    }

    #[tokio::test]
    async fn unregistered_custom_provider_errors_before_http() {
        let gateway = LlmGateway::new();
        gateway
            .register_profile(profile("p1", LlmProvider::Custom("nope".to_string())))
            .unwrap();

        let err = gateway.generate(&request("p1")).await.unwrap_err();
        assert!(matches!(err, LlmError::FormatterNotFound(_)));
    }
}

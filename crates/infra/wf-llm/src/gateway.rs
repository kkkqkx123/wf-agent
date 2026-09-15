use std::sync::Arc;

use dashmap::DashMap;
use wf_metrics::collectors::TokenMetricsCollector;
use wf_types::llm::{LlmProfile, LlmRequest, LlmResult as LlmResponseType};

pub mod merge;

use crate::client::LlmClient;
use crate::client::LlmClientImpl;
use crate::config::catalog::ModelCatalog;
use crate::config::profile::ProfileManager;
use crate::config::provider::{apply_provider_defaults, ProviderDefinitionRegistry};
use crate::error::{LlmError, LlmResult};
use crate::messaging::stream::MessageStream;
use crate::registry::CodecRegistry;

/// Assembled request ready for dispatch: the resolved profile snapshot,
/// the merged effective request and the cached client bound to them.
/// Produced only by `LlmGateway::prepare` so every entry point shares
/// a single assembly sequence.
struct PreparedRequest {
    profile: LlmProfile,
    effective: LlmRequest,
    client: Arc<LlmClientImpl>,
}

/// Single facade for all LLM calls.
///
/// Responsibilities:
/// - resolve the profile for a mandatory `profile_id` (no fallback branch)
/// - route to mock clients (test injection) or real clients
/// - resolve codecs through the registry (built-ins + runtime custom)
/// - record token usage metrics for both generate and stream paths
///
/// Request merging delegates to `gateway::merge`; stream metrics wrapping
/// delegates to `token::stream`.
#[derive(Clone)]
pub struct LlmGateway {
    clients: Arc<DashMap<String, Arc<LlmClientImpl>>>,
    profiles: ProfileManager,
    codecs: CodecRegistry,
    providers: ProviderDefinitionRegistry,
    model_catalog: ModelCatalog,
    #[cfg(feature = "mock")]
    mock_clients: Arc<DashMap<String, Arc<crate::mock::MockLlmClient>>>,
    token_metrics: Option<TokenMetricsCollector>,
}

impl LlmGateway {
    pub fn new() -> Self {
        Self::new_with_codec_registry(CodecRegistry::new())
    }

    /// Create a gateway with a caller-provided codec registry (custom
    /// formats must be registered on the registry before first use).
    pub fn new_with_codec_registry(codecs: CodecRegistry) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            profiles: ProfileManager::new(),
            codecs,
            providers: ProviderDefinitionRegistry::new(),
            model_catalog: ModelCatalog::new(),
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
        let effective = apply_provider_defaults(profile, &self.providers)?;
        let profile_id = effective.id.clone();
        self.profiles.register(effective)?;
        let prefix = format!("{profile_id}::");
        self.clients.retain(|key, _| !key.starts_with(&prefix));
        Ok(())
    }

    /// Register a provider definition. All cached clients are evicted so
    /// profiles referencing the definition pick up the new defaults.
    pub fn register_provider_definition(
        &self,
        definition: wf_types::llm::LlmProviderDefinition,
    ) -> LlmResult<()> {
        self.providers.register(definition)?;
        self.clients.clear();
        Ok(())
    }

    /// Remove a provider definition and evict all cached clients. Profiles
    /// referencing the removed definition keep their merged snapshot.
    pub fn remove_provider_definition(
        &self,
        id: &str,
    ) -> Option<wf_types::llm::LlmProviderDefinition> {
        let removed = self.providers.remove(id);
        if removed.is_some() {
            self.clients.clear();
        }
        removed
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

    /// The codec registry: register custom formats here before the
    /// first request that uses them.
    pub fn codec_registry(&self) -> &CodecRegistry {
        &self.codecs
    }

    /// The provider definition registry backing `LlmProfile::provider_id`.
    pub fn provider_registry(&self) -> &ProviderDefinitionRegistry {
        &self.providers
    }

    /// Off-hot-path model listing over a provider definition.
    pub fn model_catalog(&self) -> &ModelCatalog {
        &self.model_catalog
    }

    pub fn has_profile(&self, id: &str) -> bool {
        self.profiles.has(id)
    }

    /// Remove a profile and evict all cached clients bound to it. Returns the
    /// removed profile.
    pub fn remove_profile(&self, id: &str) -> Option<LlmProfile> {
        let removed = self.profiles.remove(id);
        if removed.is_some() {
            let prefix = format!("{id}::");
            self.clients.retain(|key, _| !key.starts_with(&prefix));
        }
        removed
    }

    /// Clear all profiles, provider definitions and the client cache.
    pub fn clear_all(&self) {
        self.profiles.clear();
        self.providers.clear();
        self.clients.clear();
    }

    pub async fn generate(
        &self,
        request: &LlmRequest,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> LlmResult<LlmResponseType> {
        #[cfg(feature = "mock")]
        if let Some(client) = self.mock_client(&request.profile_id) {
            return client.generate(request, cancel).await;
        }

        let prepared = self.prepare(request)?;
        let result = prepared
            .client
            .generate(&prepared.effective, cancel)
            .await?;
        self.record_token_usage(&result, &prepared.profile);
        Ok(result)
    }

    pub async fn generate_stream(
        &self,
        request: &LlmRequest,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> LlmResult<Box<dyn MessageStream>> {
        #[cfg(feature = "mock")]
        if let Some(client) = self.mock_client(&request.profile_id) {
            return client.generate_stream(request, cancel).await;
        }

        let prepared = self.prepare(request)?;
        let stream = prepared
            .client
            .generate_stream(&prepared.effective, cancel)
            .await?;
        Ok(Box::new(crate::token::stream::TokenRecordingStream::new(
            stream,
            self.token_metrics.clone(),
            prepared.profile.model.clone(),
        )))
    }

    pub async fn count_tokens(
        &self,
        request: &LlmRequest,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> LlmResult<wf_types::llm::TokenCountResult> {
        #[cfg(feature = "mock")]
        if let Some(client) = self.mock_client(&request.profile_id) {
            return client.count_tokens(request, cancel).await;
        }

        let prepared = self.prepare(request)?;
        prepared
            .client
            .count_tokens(&prepared.effective, cancel)
            .await
    }

    /// Single assembly preamble shared by all request entry points:
    /// resolve the profile, merge request overrides, then fetch the client.
    /// Mock routing and result post-processing stay in each caller.
    fn prepare(&self, request: &LlmRequest) -> LlmResult<PreparedRequest> {
        let profile = self.resolve_profile(&request.profile_id)?;
        let effective = merge::merge_request(request, &profile)?;
        let client = self.get_or_create_client(&profile)?;
        Ok(PreparedRequest {
            profile,
            effective,
            client,
        })
    }

    fn resolve_profile(&self, profile_id: &str) -> LlmResult<LlmProfile> {
        if profile_id.is_empty() {
            return self.profiles.get_default().ok_or_else(|| {
                LlmError::ProfileNotFound("(default profile not registered)".to_string())
            });
        }
        self.profiles
            .get(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(profile_id.to_string()))
    }

    fn get_or_create_client(&self, profile: &LlmProfile) -> LlmResult<Arc<LlmClientImpl>> {
        let key = format!("{}::{}", profile.id, profile.model);

        if let Some(client) = self.clients.get(key.as_str()) {
            return Ok(client.clone());
        }

        let codec = self.codecs.get_by_format(&profile.format)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                profile.timeout.unwrap_or(60),
            ))
            .build()
            .unwrap_or_default();

        let client_impl = Arc::new(LlmClientImpl::new(client, codec, profile.clone()));
        self.clients.insert(key, client_impl.clone());
        Ok(client_impl)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::LlmCodec;
    use crate::error::LlmError;
    use crate::registry::CodecRegistry;
    use wf_types::llm::{LlmFormat, LlmProfile, LlmRequest, MessageStreamEvent};
    use wf_types::tool::Tool;

    /// A codec whose `build_request` fails with a distinctive error, used
    /// to prove the gateway resolved the *custom* codec from the registry.
    struct ProbeCodec;

    impl LlmCodec for ProbeCodec {
        fn build_request(
            &self,
            _request: &LlmRequest,
            _profile: &LlmProfile,
        ) -> LlmResult<reqwest::Request> {
            Err(LlmError::ConfigError("custom codec engaged".to_string()))
        }

        fn parse_response(&self, _body: &str, _request: &LlmRequest) -> LlmResult<LlmResponseType> {
            Err(LlmError::ConfigError("custom codec engaged".to_string()))
        }

        fn parse_stream_chunk(&self, _data: &str) -> LlmResult<Option<MessageStreamEvent>> {
            Ok(None)
        }

        fn convert_tools(&self, _tools: &[Tool]) -> LlmResult<Vec<serde_json::Value>> {
            Ok(Vec::new())
        }

        fn parse_tool_calls(
            &self,
            _result: &LlmResponseType,
        ) -> Vec<wf_types::message::LlmToolCall> {
            Vec::new()
        }
    }

    fn profile(id: &str, format: LlmFormat) -> LlmProfile {
        LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            format,
            provider_id: None,
            model: "custom-model".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        }
    }

    fn request(profile_id: &str) -> LlmRequest {
        LlmRequest {
            profile_id: profile_id.to_string(),
            messages: Vec::new(),
            parameters: None,
            generation: None,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        }
    }

    #[tokio::test]
    async fn custom_codec_resolved_via_registry() {
        let registry = CodecRegistry::new();
        registry
            .register("my_custom_provider", Arc::new(ProbeCodec))
            .expect("custom registration must succeed");
        let gateway = LlmGateway::new_with_codec_registry(registry);
        gateway
            .register_profile(profile(
                "p1",
                LlmFormat::Custom("my_custom_provider".to_string()),
            ))
            .unwrap();

        let err = gateway.generate(&request("p1"), None).await.unwrap_err();
        assert!(
            err.to_string().contains("custom codec engaged"),
            "custom codec must have been selected: {}",
            err
        );
    }

    #[tokio::test]
    async fn unregistered_custom_format_errors_before_http() {
        let gateway = LlmGateway::new();
        gateway
            .register_profile(profile("p1", LlmFormat::Custom("nope".to_string())))
            .unwrap();

        let err = gateway.generate(&request("p1"), None).await.unwrap_err();
        assert!(matches!(err, LlmError::CodecNotFound(_)));
    }

    #[tokio::test]
    async fn empty_profile_id_resolves_to_default() {
        let registry = CodecRegistry::new();
        registry
            .register("probe", Arc::new(ProbeCodec))
            .expect("registration must succeed");
        let gateway = LlmGateway::new_with_codec_registry(registry);
        gateway
            .register_profile(profile("p1", LlmFormat::Custom("probe".to_string())))
            .unwrap();

        // The custom codec fails inside `build_request` before any HTTP;
        // reaching it proves the default profile was resolved.
        let err = gateway.generate(&request(""), None).await.unwrap_err();
        assert!(
            err.to_string().contains("custom codec engaged"),
            "empty id must resolve the default profile: {err}"
        );
    }

    #[tokio::test]
    async fn empty_profile_id_errors_without_default() {
        let gateway = LlmGateway::new();
        let err = gateway.generate(&request(""), None).await.unwrap_err();
        assert!(matches!(err, LlmError::ProfileNotFound(_)));
    }

    #[test]
    fn remove_profile_evicts_cached_clients() {
        let gateway = LlmGateway::new();
        gateway
            .register_profile(profile("p1", LlmFormat::OpenaiChat))
            .unwrap();
        gateway
            .register_profile(profile("p2", LlmFormat::OpenaiChat))
            .unwrap();

        let client = gateway
            .get_or_create_client(&gateway.profiles.get("p1").unwrap())
            .unwrap();
        let key = format!("{}::{}", client.profile().id, client.profile().model);
        assert!(gateway.clients.contains_key(key.as_str()));

        assert!(gateway.remove_profile("p1").is_some());
        assert!(
            !gateway.clients.contains_key(key.as_str()),
            "clients of the removed profile must be evicted"
        );
        assert!(!gateway.has_profile("p1"));
        assert!(gateway.has_profile("p2"));
    }

    #[test]
    fn remove_profile_preserves_other_clients() {
        let gateway = LlmGateway::new();
        gateway
            .register_profile(profile("p1", LlmFormat::OpenaiChat))
            .unwrap();
        gateway
            .register_profile(profile("p2", LlmFormat::OpenaiChat))
            .unwrap();

        let client = gateway
            .get_or_create_client(&gateway.profiles.get("p2").unwrap())
            .unwrap();
        let key = format!("{}::{}", client.profile().id, client.profile().model);

        gateway.remove_profile("p1");
        assert!(
            gateway.clients.contains_key(key.as_str()),
            "unrelated clients must survive"
        );
    }

    fn provider_definition() -> wf_types::llm::LlmProviderDefinition {
        wf_types::llm::LlmProviderDefinition {
            id: "acme".to_string(),
            name: None,
            description: None,
            base_url: Some("https://api.acme.test".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: None,
            api_version: None,
            metadata: None,
        }
    }

    #[test]
    fn register_profile_merges_provider_defaults() {
        let gateway = LlmGateway::new();
        gateway
            .register_provider_definition(provider_definition())
            .unwrap();

        let mut pending = profile("p1", LlmFormat::OpenaiChat);
        pending.provider_id = Some("acme".to_string());
        gateway.register_profile(pending).unwrap();

        let stored = gateway.profiles.get("p1").unwrap();
        assert_eq!(stored.base_url.as_deref(), Some("https://api.acme.test"));
        assert_eq!(stored.auth_type.as_deref(), Some("bearer"));
    }

    #[test]
    fn register_profile_with_unknown_provider_fails() {
        let gateway = LlmGateway::new();
        let mut pending = profile("p1", LlmFormat::OpenaiChat);
        pending.provider_id = Some("nope".to_string());
        assert!(gateway.register_profile(pending).is_err());
    }

    #[test]
    fn provider_definition_change_evicts_clients() {
        let gateway = LlmGateway::new();
        gateway
            .register_provider_definition(provider_definition())
            .unwrap();
        let mut pending = profile("p1", LlmFormat::OpenaiChat);
        pending.provider_id = Some("acme".to_string());
        gateway.register_profile(pending).unwrap();

        let stored = gateway.profiles.get("p1").unwrap();
        let client = gateway.get_or_create_client(&stored).unwrap();
        let key = format!("{}::{}", client.profile().id, client.profile().model);
        assert!(gateway.clients.contains_key(key.as_str()));

        gateway
            .register_provider_definition(provider_definition())
            .unwrap();
        assert!(
            !gateway.clients.contains_key(key.as_str()),
            "provider change must evict cached clients"
        );
    }
}

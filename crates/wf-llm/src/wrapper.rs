use crate::client::LlmClient;
use crate::client_factory::ClientFactory;
use crate::error::{LlmError, LlmResult};
use wf_metrics::collectors::TokenMetricsCollector;
use wf_types::llm::{LlmRequest, LlmResult as LlmResponseType};

#[derive(Clone)]
pub struct LlmWrapper {
    factory: ClientFactory,
    token_metrics: Option<TokenMetricsCollector>,
}

impl LlmWrapper {
    pub fn new() -> Self {
        Self {
            factory: ClientFactory::new(),
            token_metrics: None,
        }
    }

    pub fn with_factory(factory: ClientFactory) -> Self {
        Self {
            factory,
            token_metrics: None,
        }
    }

    /// Attach an optional token usage collector (zero overhead when absent).
    pub fn with_token_metrics(mut self, token_metrics: TokenMetricsCollector) -> Self {
        self.token_metrics = Some(token_metrics);
        self
    }

    pub fn factory(&self) -> &ClientFactory {
        &self.factory
    }

    pub async fn generate(&self, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        let profile_id = request.profile_id.as_deref();
        let profile = self.factory.get_profile(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(
                profile_id.unwrap_or("default").to_string()
            ))?;

        let client = self.factory.get_or_create(&profile);
        let result = client.generate(request).await?;
        self.record_token_usage(&result, &profile);
        Ok(result)
    }

    pub async fn generate_stream(&self, request: &LlmRequest) -> LlmResult<Box<dyn crate::message_stream::MessageStream>> {
        let profile_id = request.profile_id.as_deref();
        let profile = self.factory.get_profile(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(
                profile_id.unwrap_or("default").to_string()
            ))?;

        let client = self.factory.get_or_create(&profile);
        client.generate_stream(request).await
    }

    pub async fn count_tokens(&self, request: &LlmRequest) -> LlmResult<u32> {
        let profile_id = request.profile_id.as_deref();
        let profile = self.factory.get_profile(profile_id)
            .ok_or_else(|| LlmError::ProfileNotFound(
                profile_id.unwrap_or("default").to_string()
            ))?;

        let client = self.factory.get_or_create(&profile);
        client.count_tokens(request).await
    }

    fn record_token_usage(&self, result: &LlmResponseType, profile: &wf_types::llm::LlmProfile) {
        let Some(collector) = self.token_metrics.as_ref() else { return };
        let Some(usage) = result.usage.as_ref() else { return };
        collector.record_token_usage(
            usage.prompt_tokens as u64,
            usage.completion_tokens as u64,
            usage.total_cost,
            Some(&profile.model),
        );
    }
}

impl Default for LlmWrapper {
    fn default() -> Self {
        Self::new()
    }
}

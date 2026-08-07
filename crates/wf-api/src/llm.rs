use std::sync::Arc;

use futures::future::join_all;

use wf_llm::MessageStream;
use wf_types::llm::{LlmRequest, LlmResult, TokenCountResult};

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// Direct LLM execution entry points (TS `GenerateCommand` /
/// `GenerateBatchCommand` counterparts).
///
/// Everything runs through the shared `LlmGateway` of the context, so
/// profiles registered via `LlmProfileApi` are resolved exactly as they are
/// during workflow / agent executions. Requests that reference an unknown
/// profile surface `ApiError::NotFound`; malformed parameters are rejected
/// with `ApiError::Validation`.
pub struct LlmApi {
    ctx: Arc<ApiContext>,
}

impl LlmApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Run a single LLM generation request.
    pub async fn generate(&self, request: &LlmRequest) -> ApiResult<LlmResult> {
        if request.messages.is_empty() {
            return Err(ApiError::Validation(
                "LLM request must contain at least one message".into(),
            ));
        }
        self.ctx.llm_gateway.generate(request).await.map_err(Into::into)
    }

    /// Run several LLM requests in parallel; fails fast on the first error.
    pub async fn generate_batch(&self, requests: &[LlmRequest]) -> ApiResult<Vec<LlmResult>> {
        if requests.is_empty() {
            return Err(ApiError::Validation(
                "LLM batch request list must not be empty".into(),
            ));
        }
        for (i, request) in requests.iter().enumerate() {
            if request.messages.is_empty() {
                return Err(ApiError::Validation(format!(
                    "LLM request {i} must contain at least one message"
                )));
            }
        }
        let results = join_all(
            requests
                .iter()
                .map(|request| self.ctx.llm_gateway.generate(request)),
        )
        .await;
        let mut out = Vec::with_capacity(results.len());
        for result in results {
            out.push(result.map_err(ApiError::from)?);
        }
        Ok(out)
    }

    /// Start a streaming LLM generation. The caller consumes `MessageStream`
    /// events (text deltas / final message / end).
    pub async fn generate_stream(
        &self,
        request: &LlmRequest,
    ) -> ApiResult<Box<dyn MessageStream>> {
        if request.messages.is_empty() {
            return Err(ApiError::Validation(
                "LLM request must contain at least one message".into(),
            ));
        }
        self.ctx
            .llm_gateway
            .generate_stream(request)
            .await
            .map_err(Into::into)
    }

    /// Token count of a request without executing it (mirrors the TS
    /// `countTokens`).
    pub async fn count_tokens(&self, request: &LlmRequest) -> ApiResult<TokenCountResult> {
        self.ctx
            .llm_gateway
            .count_tokens(request)
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_llm::{LlmResponseSpec, MockLlmClient};
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::llm::LlmProvider;
    use wf_types::message::{Message, MessageContentValue, MessageRole};

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn user_message(text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn request(profile_id: &str, text: &str) -> LlmRequest {
        LlmRequest {
            profile_id: profile_id.to_string(),
            messages: vec![user_message(text)],
            parameters: None,
            tools: None,
            tool_call_format: None,
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: Some("exec-llm".into()),
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        }
    }

    fn mock_profile(id: &str) -> wf_types::llm::LlmProfile {
        wf_types::llm::LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            provider: LlmProvider::OpenaiChat,
            model: "mock-model".to_string(),
            api_key: Some("sk-test".into()),
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

    #[tokio::test]
    async fn generate_serves_mock_response() {
        let ctx = make_ctx();
        let mock = Arc::new(MockLlmClient::new());
        mock.script(LlmResponseSpec::text("hello from mock"));
        ctx.llm_gateway
            .register_mock("mock-profile", mock.clone());
        ctx.llm_gateway
            .register_profile(mock_profile("mock-profile"))
            .unwrap();

        let api = LlmApi::new(ctx);
        let result = api.generate(&request("mock-profile", "hi")).await.unwrap();
        assert_eq!(result.content.as_deref(), Some("hello from mock"));
        assert_eq!(mock.recorded_count(), 1);
    }

    #[tokio::test]
    async fn generate_batch_runs_all_requests() {
        let ctx = make_ctx();
        let mock = Arc::new(MockLlmClient::new());
        mock.script(LlmResponseSpec::text("first"));
        mock.script(LlmResponseSpec::text("second"));
        ctx.llm_gateway.register_mock("mock-batch", mock.clone());
        ctx.llm_gateway
            .register_profile(mock_profile("mock-batch"))
            .unwrap();

        let api = LlmApi::new(ctx);
        let results = api
            .generate_batch(&[
                request("mock-batch", "a"),
                request("mock-batch", "b"),
            ])
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].content.as_deref(), Some("first"));
        assert_eq!(results[1].content.as_deref(), Some("second"));
        assert_eq!(mock.recorded_count(), 2);
    }

    #[tokio::test]
    async fn unknown_profile_is_not_found() {
        let ctx = make_ctx();
        let api = LlmApi::new(ctx);
        let err = api.generate(&request("no-such-profile", "hi")).await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn empty_messages_are_rejected() {
        let ctx = make_ctx();
        let api = LlmApi::new(ctx);
        let mut req = request("mock-profile", "hi");
        req.messages.clear();
        let err = api.generate(&req).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));

        let err = api.generate_batch(&[]).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn generate_stream_produces_final_message() {
        let ctx = make_ctx();
        let mock = Arc::new(MockLlmClient::new());
        mock.script(LlmResponseSpec::text("streamed reply"));
        ctx.llm_gateway.register_mock("mock-stream", mock.clone());
        ctx.llm_gateway
            .register_profile(mock_profile("mock-stream"))
            .unwrap();

        let api = LlmApi::new(ctx);
        let mut stream = api
            .generate_stream(&request("mock-stream", "hi"))
            .await
            .unwrap();

        let mut saw_final = false;
        while let Some(event) = stream.next().await {
            if let Ok(wf_types::llm::MessageStreamEvent::FinalMessage(msg)) = event {
                assert_eq!(
                    msg.message.content,
                    MessageContentValue::Text("streamed reply".to_string())
                );
                saw_final = true;
            }
        }
        assert!(saw_final, "stream must end with a final message");
    }
}

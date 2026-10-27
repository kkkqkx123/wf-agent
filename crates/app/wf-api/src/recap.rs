//! Screen-only recap reading face: project the stored conversation,
//! run one tool-free side call with the caller-declared profile, clean the
//! answer. Read-only: no log writes, no events, no state changes. Every
//! runtime failure converges to `Ok(None)`; only caller misuse (blank ids)
//! reports a validation error.

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Parameters of one recap read. The profile is caller-declared: the side
/// call must read the session ledger with the profile that ran it, never a
/// lightweight fallback, so an absent profile is rejected instead of
/// substituted.
#[derive(Debug, Clone)]
pub struct RecapParams {
    /// Session anchor (chat `session_id` or agent loop id). Both share the
    /// message store keyed by agent loop id.
    pub session_id: String,
    /// LLM profile id that ran the session and serves the side call.
    pub profile_id: String,
}

/// Read one recap for `params`. Returns `Ok(None)` when there is nothing
/// worth saying or when the side call fails, times out or is cancelled.
pub async fn recap(ctx: &ApiContext, params: RecapParams) -> ApiResult<Option<String>> {
    recap_with_cancel(ctx, params, None).await
}

/// Recap read with an optional caller cancellation token. Cancellation only
/// ends the bypass; the completed turn is never affected.
pub async fn recap_with_cancel(
    ctx: &ApiContext,
    params: RecapParams,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> ApiResult<Option<String>> {
    let session_id = params.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err(ApiError::Validation(
            "recap requires a session id".to_string(),
        ));
    }
    let profile_id = params.profile_id.trim().to_string();
    if profile_id.is_empty() {
        return Err(ApiError::Validation(
            "recap requires a profile id".to_string(),
        ));
    }
    let history =
        match crate::agent::agent_message::conversation_history(ctx, &session_id, None).await {
            Ok(history) => history,
            Err(error) => {
                tracing::debug!(
                    target: "wf_api",
                    session_id = %session_id,
                    error = %error,
                    "recap history read failed; returning no recap"
                );
                return Ok(None);
            }
        };
    Ok(
        wf_execution_shared::recap::request_recap(&ctx.llm_gateway, &profile_id, &history, cancel)
            .await,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::message::{MessageContentValue, MessageRole};
    use wf_types::MessageStorageMetadata;

    fn mock_profile(id: &str) -> wf_types::llm::LlmProfile {
        wf_types::llm::LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            format: wf_types::llm::LlmFormat::OpenaiChat,
            provider_id: None,
            model: "mock-model".to_string(),
            api_key: Some("sk-test".into()),
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
            proxy: None,
            no_proxy: None,
            circuit_breaker: None,
            prompt_caching: None,
            auth: None,
        }
    }

    fn make_ctx() -> (Arc<ApiContext>, Arc<wf_llm::MockLlmClient>) {
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::text(
            "Finished the auth route; next waits on your e2e call.",
        ));
        let gateway = Arc::new(wf_llm::LlmGateway::new());
        gateway.register_mock("mock", mock.clone());
        gateway.register_profile(mock_profile("mock")).unwrap();
        let ctx = ApiContext::from_runtime_parts(
            Arc::new(StorageContext::new_memory()),
            Arc::new(ResourceRegistries::new()),
            Arc::new(wf_core::EventBus::new(64)),
            gateway,
            Arc::new(wf_tools::create_default_tool_registry()),
            None,
        );
        (Arc::new(ctx), mock)
    }

    fn message(id: &str, role: MessageRole, text: &str, ts: i64) -> wf_types::message::Message {
        wf_types::message::Message {
            id: id.into(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: ts,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }
    }

    async fn seed(ctx: &ApiContext, session: &str) {
        ctx.storage
            .message
            .save(&MessageStorageMetadata {
                id: "r1".into(),
                execution_id: session.into(),
                agent_loop_id: Some(session.into()),
                message: message("m1", MessageRole::User, "fix the auth route", 100),
            })
            .await
            .unwrap();
        ctx.storage
            .message
            .save(&MessageStorageMetadata {
                id: "r2".into(),
                execution_id: session.into(),
                agent_loop_id: Some(session.into()),
                message: message(
                    "m2",
                    MessageRole::Assistant,
                    "done, two tests pass; next is your e2e call",
                    200,
                ),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn one_recap_reads_history_and_issues_one_request() {
        let (ctx, mock) = make_ctx();
        seed(&ctx, "session-1").await;
        let out = recap(
            &ctx,
            RecapParams {
                session_id: "session-1".to_string(),
                profile_id: "mock".to_string(),
            },
        )
        .await
        .expect("recap read runs")
        .expect("recap produced");
        assert!(!out.is_empty());
        assert_eq!(mock.recorded_count(), 1);
        assert_eq!(mock.last_request().unwrap().profile_id, "mock");
    }

    #[tokio::test]
    async fn empty_history_is_no_recap_without_a_request() {
        let (ctx, mock) = make_ctx();
        let out = recap(
            &ctx,
            RecapParams {
                session_id: "missing-session".to_string(),
                profile_id: "mock".to_string(),
            },
        )
        .await
        .expect("recap read runs");
        assert_eq!(out, None);
        assert_eq!(mock.recorded_count(), 0);
    }

    #[tokio::test]
    async fn blank_ids_are_caller_errors() {
        let (ctx, _) = make_ctx();
        let err = recap(
            &ctx,
            RecapParams {
                session_id: "  ".to_string(),
                profile_id: "mock".to_string(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
        let err = recap(
            &ctx,
            RecapParams {
                session_id: "s".to_string(),
                profile_id: "  ".to_string(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
    }
}

//! Tool-free multi-turn chat over a caller-owned session.
//!
//! The light primitive between a stateless single generation and the full
//! agent loop: the caller owns the session, each turn appends the user text
//! and records the assistant message of the generation. There is no tool
//! execution, no checkpointing, no approval, no event publishing and no
//! execution-record persistence. The request template carries no tools
//! field, so a tool-carrying chat request cannot be constructed.

use wf_llm::{messaging::message_builder::user_text, LlmGateway};
use wf_types::llm::{LlmGenerationParams, LlmRequest, LlmResult};
use wf_types::message::Message;

use crate::conversation_session::ConversationSession;
use crate::error::{ExecutionSharedError, ExecutionSharedResult};

/// Per-round request template for a chat session. Intentionally mirrors
/// only the tool-free subset of a generation request.
#[derive(Debug, Clone, Default)]
pub struct ChatTemplate {
    pub profile_id: String,
    pub parameters: Option<serde_json::Value>,
    pub generation: Option<LlmGenerationParams>,
}

/// Caller-owned multi-turn chat session. All state lives here; nothing is
/// shared with the agent loop or the checkpoint machinery.
pub struct ChatSession {
    template: ChatTemplate,
    conversation: ConversationSession,
}

impl ChatSession {
    pub fn new(profile_id: String) -> Self {
        Self::with_template(ChatTemplate {
            profile_id,
            parameters: None,
            generation: None,
        })
    }

    pub fn with_template(template: ChatTemplate) -> Self {
        Self {
            template,
            conversation: ConversationSession::new(),
        }
    }

    pub fn template(&self) -> &ChatTemplate {
        &self.template
    }

    /// Send one user turn and record the assistant reply.
    ///
    /// Fails on blank text or on a missing profile id. Provider-reported
    /// token usage is folded into the session when present.
    pub async fn send(
        &mut self,
        gateway: &LlmGateway,
        text: &str,
        execution_id: Option<&str>,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> ExecutionSharedResult<LlmResult> {
        if text.trim().is_empty() {
            return Err(ExecutionSharedError::Internal(
                "chat send requires non-empty text".to_string(),
            ));
        }
        if self.template.profile_id.is_empty() {
            return Err(ExecutionSharedError::Internal(
                "chat send requires a profile id".to_string(),
            ));
        }
        self.conversation.add_message(user_text(text));
        let request = LlmRequest {
            profile_id: self.template.profile_id.clone(),
            messages: self.conversation.view_messages(),
            parameters: self.template.parameters.clone(),
            generation: self.template.generation.clone(),
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: execution_id.map(str::to_string),
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
            timeout_ms: None,
        };
        let result = gateway
            .generate(&request, cancel)
            .await
            .map_err(|e| ExecutionSharedError::Internal(format!("chat generation failed: {e}")))?;
        self.conversation.add_message(result.message.clone());
        if let Some(usage) = result.usage.as_ref() {
            self.conversation.update_token_usage(usage);
        }
        self.conversation.finalize_current_request();
        Ok(result)
    }

    /// Full append-only history of the session.
    pub fn history(&self) -> &[Message] {
        self.conversation.history()
    }

    /// Projected messages actually sent to the model.
    pub fn view_messages(&self) -> Vec<Message> {
        self.conversation.view_messages()
    }

    /// Cumulative token usage reported by the provider across turns.
    pub fn token_usage(&self) -> u64 {
        self.conversation.token_usage()
    }

    /// Drop all history and usage tracking.
    pub fn reset(&mut self) {
        self.conversation.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
        }
    }

    fn gateway_with_texts(profile: &str, texts: &[&str]) -> LlmGateway {
        let gateway = LlmGateway::new();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        for text in texts {
            mock.script(wf_llm::LlmResponseSpec::text(*text));
        }
        gateway.register_mock(profile, mock);
        gateway.register_profile(mock_profile(profile)).unwrap();
        gateway
    }

    #[tokio::test]
    async fn consecutive_turns_accumulate_history_in_order() {
        let gateway = gateway_with_texts("mock-chat", &["first", "second"]);
        let mut session = ChatSession::new("mock-chat".to_string());

        let first = session.send(&gateway, "hello", None, None).await.unwrap();
        assert_eq!(first.content.as_deref(), Some("first"));
        assert_eq!(session.history().len(), 2);

        let second = session
            .send(&gateway, "again", Some("exec-chat"), None)
            .await
            .unwrap();
        assert_eq!(second.content.as_deref(), Some("second"));
        assert_eq!(session.history().len(), 4);
        assert_eq!(session.view_messages().len(), 4);
        let recorded = session.history()[3].clone();
        assert_eq!(recorded, second.message);
    }

    #[tokio::test]
    async fn blank_text_is_rejected_without_recording() {
        let gateway = gateway_with_texts("mock-chat-blank", &["unused"]);
        let mut session = ChatSession::new("mock-chat-blank".to_string());
        let err = session.send(&gateway, "   ", None, None).await.unwrap_err();
        assert!(err.to_string().contains("non-empty"));
        assert!(session.history().is_empty());
    }

    #[tokio::test]
    async fn missing_profile_is_rejected() {
        let gateway = gateway_with_texts("mock-chat-noprofile", &["unused"]);
        let mut session = ChatSession::new(String::new());
        let err = session
            .send(&gateway, "hello", None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("profile id"));
    }

    #[tokio::test]
    async fn reset_clears_history() {
        let gateway = gateway_with_texts("mock-chat-reset", &["reply"]);
        let mut session = ChatSession::new("mock-chat-reset".to_string());
        session.send(&gateway, "hello", None, None).await.unwrap();
        assert_eq!(session.history().len(), 2);
        session.reset();
        assert!(session.history().is_empty());
        assert_eq!(session.token_usage(), 0);
    }
}

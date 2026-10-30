//! Tool-free multi-turn chat over a caller-owned session.
//!
//! The light primitive between a stateless single generation and the full
//! agent loop: the caller owns the session, each turn appends the user text
//! and records the assistant message of the generation. There is no tool
//! execution, no checkpointing, no approval, no event publishing and no
//! execution-record persistence. The request template carries no tools
//! field, so a tool-carrying chat request cannot be constructed.

use wf_llm::{message_builder::user_text, LlmGateway};
use wf_types::llm::{LlmGenerationParams, LlmRequest, LlmResult, TokenUsageStats};
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

    /// Seed the session with an existing conversation, e.g. the messages a
    /// persisted session was restored from. The seeded messages become the
    /// history the next turn continues.
    pub fn restore(&mut self, messages: Vec<Message>) {
        for message in messages {
            self.conversation.add_message(message);
        }
    }

    /// Append the user turn and build the tool-free request for it.
    ///
    /// Fails on blank text or on a missing profile id. The request carries
    /// no tools, so the model cannot call back into the host.
    pub fn begin_turn(
        &mut self,
        text: &str,
        execution_id: Option<&str>,
    ) -> ExecutionSharedResult<LlmRequest> {
        if text.trim().is_empty() {
            return Err(ExecutionSharedError::Internal(
                "chat turn requires non-empty text".to_string(),
            ));
        }
        if self.template.profile_id.is_empty() {
            return Err(ExecutionSharedError::Internal(
                "chat turn requires a profile id".to_string(),
            ));
        }
        self.conversation.add_message(user_text(text));
        Ok(LlmRequest {
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
            prompt_caching: None,
            cached_content: None,
            max_retries: None,
            retry_delay_ms: None,
            extra_headers: None,
        })
    }

    /// Record the assistant reply produced for the prepared turn.
    ///
    /// Provider-reported token usage is folded into the session when present.
    pub fn record_reply(&mut self, message: Message, usage: Option<&TokenUsageStats>) {
        self.conversation.add_message(message);
        if let Some(usage) = usage {
            self.conversation.update_token_usage(usage);
        }
        self.conversation.finalize_current_request();
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
        let request = self.begin_turn(text, execution_id)?;
        let result = gateway
            .generate(&request, cancel)
            .await
            .map_err(|e| ExecutionSharedError::Internal(format!("chat generation failed: {e}")))?;
        self.record_reply(result.message.clone(), result.usage.as_ref());
        Ok(result)
    }

    /// Move the session to another profile, migrating the history once.
    ///
    /// Staying on the same model is a clone-level no-op; a genuine move
    /// drops foreign thinking signatures and native blocks so the next
    /// provider never sees them. Fails on a blank profile id and leaves
    /// the session untouched.
    pub fn switch_profile(
        &mut self,
        gateway: &LlmGateway,
        new_profile_id: &str,
    ) -> ExecutionSharedResult<()> {
        if new_profile_id.trim().is_empty() {
            return Err(ExecutionSharedError::Internal(
                "profile switch requires a non-empty profile id".to_string(),
            ));
        }
        let supports_image = gateway
            .profile_registry()
            .get(new_profile_id)
            .and_then(|profile| wf_llm::model_metadata(&profile.model))
            .map(|metadata| metadata.supports_image())
            .unwrap_or(true);
        let migrated = gateway.migrate_history_for_profile_with_image_support(
            self.conversation.history(),
            Some(self.template.profile_id.as_str()),
            new_profile_id,
            supports_image,
        );
        let view = self.conversation.active_view().clone();
        self.conversation.restore_history(migrated, view);
        self.template.profile_id = new_profile_id.to_string();
        Ok(())
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
    use wf_types::llm::LlmFormat;

    fn mock_profile(id: &str) -> wf_types::llm::LlmProfile {
        wf_types::llm::LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            format: LlmFormat::OpenaiChat,
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
            metadata: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
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

    #[test]
    fn restored_history_continues_the_conversation() {
        let mut session = ChatSession::new("mock-chat".to_string());
        session.restore(vec![user_text("earlier question")]);
        assert_eq!(session.history().len(), 1);
        let request = session.begin_turn("next question", None).expect("request");
        assert_eq!(request.messages.len(), 2);
        assert!(request.tools.is_none());
    }

    #[test]
    fn a_turn_never_carries_tools() {
        let mut session = ChatSession::new("mock-chat".to_string());
        let request = session.begin_turn("hello", None).expect("request");
        assert!(request.tools.is_none());
        assert!(request.tool_call_protocol.is_none());
    }

    #[test]
    fn switch_profile_rewrites_history_for_the_new_model() {
        use wf_types::message::{MessageContent, MessageContentValue, MessageRole};
        let gateway = LlmGateway::new();
        gateway.register_profile(mock_profile("chat-a")).unwrap();
        let mut other = mock_profile("chat-b");
        other.model = "other-model".to_string();
        gateway.register_profile(other).unwrap();

        let mut session = ChatSession::new("chat-a".to_string());
        session.begin_turn("hello", None).expect("request");
        // Foreign thinking must not survive the move to another model.
        let thinking = wf_types::message::Message {
            id: String::new(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Rich(vec![MessageContent::Thinking {
                thinking: "hmm".to_string(),
                signature: Some("foreign-sig".to_string()),
            }]),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        };
        session.restore(vec![thinking]);
        session
            .switch_profile(&gateway, "chat-b")
            .expect("switch must succeed");
        assert_eq!(session.template().profile_id, "chat-b");
        for message in session.history() {
            if let MessageContentValue::Rich(blocks) = &message.content {
                assert!(
                    blocks
                        .iter()
                        .all(|b| !matches!(b, MessageContent::Thinking { .. })),
                    "foreign thinking must not leak"
                );
            }
        }
    }

    #[test]
    fn switch_profile_rejects_blank_ids_without_touching_history() {
        let gateway = LlmGateway::new();
        gateway.register_profile(mock_profile("chat-a")).unwrap();
        let mut session = ChatSession::new("chat-a".to_string());
        session.begin_turn("hello", None).expect("request");
        let before = session.history().to_vec();
        let err = session.switch_profile(&gateway, "   ").unwrap_err();
        assert!(err.to_string().contains("non-empty"));
        assert_eq!(session.history(), before.as_slice());
        assert_eq!(session.template().profile_id, "chat-a");
    }
}

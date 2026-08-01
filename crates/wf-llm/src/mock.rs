//! Scriptable LLM test double, compiled only with the `mock` feature.
//!
//! `MockLlmClient` implements the real `LlmClient` trait: it serves a
//! per-instance script queue (text / tool_calls / errors / stream events),
//! falls back to a stable default when the queue is exhausted, and records
//! every request (including the full message history) for test assertions.
//! Responses are built on the real `LlmResult` / `Message` types via
//! `LlmResponseSpec`, so tests exercise the same type surface as production.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use wf_types::llm::{
    LlmRequest, LlmResult as LlmResponseType, MessageStreamEnd, MessageStreamEvent,
    MessageStreamFinal, MessageStreamText, TokenUsageStats,
};
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};

use crate::client::LlmClient;
use crate::error::{LlmError, LlmResult};
use crate::message_helper::extract_text_content;
use crate::message_stream::MessageStream;

/// Builder that turns a natural-language description into a real `LlmResult`,
/// generating the matching assistant `Message` (content or tool_calls).
#[derive(Debug, Clone)]
pub struct LlmResponseSpec {
    content: Option<String>,
    tool_calls: Option<Vec<LlmToolCall>>,
    usage: Option<TokenUsageStats>,
    finish_reason: Option<String>,
    model: Option<String>,
    delay_ms: Option<u64>,
    reasoning: Option<String>,
}

impl LlmResponseSpec {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            tool_calls: None,
            usage: None,
            finish_reason: None,
            model: None,
            delay_ms: None,
            reasoning: None,
        }
    }

    pub fn tool_calls(calls: Vec<LlmToolCall>) -> Self {
        Self {
            content: None,
            tool_calls: Some(calls),
            usage: None,
            finish_reason: None,
            model: None,
            delay_ms: None,
            reasoning: None,
        }
    }

    pub fn with_usage(mut self, prompt_tokens: u32, completion_tokens: u32) -> Self {
        self.usage = Some(TokenUsageStats {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            reasoning_tokens: None,
            prompt_tokens_cost: None,
            completion_tokens_cost: None,
            total_cost: None,
        });
        self
    }

    pub fn with_finish_reason(mut self, reason: impl Into<String>) -> Self {
        self.finish_reason = Some(reason.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_delay(mut self, ms: u64) -> Self {
        self.delay_ms = Some(ms);
        self
    }

    pub fn with_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.reasoning = Some(reasoning.into());
        self
    }

    pub fn delay_ms(&self) -> Option<u64> {
        self.delay_ms
    }

    /// Build a real `LlmResult` for the given request.
    pub fn build(&self, request: &LlmRequest) -> LlmResponseType {
        let model = self
            .model
            .clone()
            .unwrap_or_else(|| request.profile_id.clone());
        let message = Message {
            id: wf_types::Id::new(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text(self.content.clone().unwrap_or_default()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: self.tool_calls.clone(),
            thinking: self.reasoning.clone(),
            metadata: None,
        };
        LlmResponseType {
            id: None,
            model,
            content: self.content.clone(),
            message,
            tool_calls: self.tool_calls.clone(),
            usage: self.usage.clone(),
            finish_reason: self.finish_reason.clone(),
            duration: 0,
            reasoning_content: self.reasoning.clone(),
            reasoning_tokens: None,
            metadata: None,
            stream_stats: None,
            warnings: None,
        }
    }

    /// Emit this spec as a stream sequence: text chunk (when present), final
    /// message, end.
    fn to_stream_events(&self, request: &LlmRequest) -> Vec<MessageStreamEvent> {
        let result = self.build(request);
        let mut events = Vec::new();
        if let Some(content) = &result.content {
            if !content.is_empty() {
                events.push(MessageStreamEvent::Text(MessageStreamText {
                    text: content.clone(),
                }));
            }
        }
        events.push(MessageStreamEvent::FinalMessage(MessageStreamFinal {
            message: result.message.clone(),
            usage: result.usage.clone(),
        }));
        events.push(MessageStreamEvent::End(MessageStreamEnd {}));
        events
    }
}

enum ScriptedResponse {
    Spec(LlmResponseSpec),
    Error(LlmError),
    Stream(Vec<MessageStreamEvent>),
}

struct MockInner {
    script: Mutex<VecDeque<ScriptedResponse>>,
    default: Mutex<Option<LlmResponseSpec>>,
    recorded: Mutex<Vec<LlmRequest>>,
    stream_delay_ms: Mutex<Option<u64>>,
}

/// Scriptable `LlmClient` implementation with request recording.
///
/// Clone shares the same script queue, default and recordings, so multiple
/// callers (or multiple nodes in one test) can share one instance.
#[derive(Clone)]
pub struct MockLlmClient {
    inner: Arc<MockInner>,
}

impl MockLlmClient {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MockInner {
                script: Mutex::new(VecDeque::new()),
                default: Mutex::new(None),
                recorded: Mutex::new(Vec::new()),
                stream_delay_ms: Mutex::new(None),
            }),
        }
    }

    /// Append a scripted response; consumed in FIFO order.
    pub fn script(&self, spec: LlmResponseSpec) {
        self.inner
            .script
            .lock()
            .unwrap()
            .push_back(ScriptedResponse::Spec(spec));
    }

    /// Append a scripted error (e.g. for retry tests).
    pub fn script_error(&self, error: LlmError) {
        self.inner
            .script
            .lock()
            .unwrap()
            .push_back(ScriptedResponse::Error(error));
    }

    /// Append a scripted stream; events are emitted in order.
    pub fn script_stream(&self, events: Vec<MessageStreamEvent>) {
        self.inner
            .script
            .lock()
            .unwrap()
            .push_back(ScriptedResponse::Stream(events));
    }

    /// Fallback response used when the script queue is exhausted.
    pub fn default(&self, spec: LlmResponseSpec) {
        *self.inner.default.lock().unwrap() = Some(spec);
    }

    /// Delay between stream events (0 disables).
    pub fn with_stream_delay(&self, ms: u64) {
        *self.inner.stream_delay_ms.lock().unwrap() = if ms == 0 { None } else { Some(ms) };
    }

    /// All recorded requests, including the full message history.
    pub fn recorded_requests(&self) -> Vec<LlmRequest> {
        self.inner.recorded.lock().unwrap().clone()
    }

    pub fn last_request(&self) -> Option<LlmRequest> {
        self.inner.recorded.lock().unwrap().last().cloned()
    }

    pub fn recorded_count(&self) -> usize {
        self.inner.recorded.lock().unwrap().len()
    }

    pub fn clear(&self) {
        self.inner.recorded.lock().unwrap().clear();
    }

    fn record(&self, request: &LlmRequest) {
        self.inner.recorded.lock().unwrap().push(request.clone());
    }

    fn pop(&self) -> Option<ScriptedResponse> {
        self.inner.script.lock().unwrap().pop_front()
    }

    fn fallback_spec(&self) -> LlmResponseSpec {
        self.inner
            .default
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| LlmResponseSpec::text(""))
    }

    fn stream_delay(&self) -> Option<Duration> {
        self.inner
            .stream_delay_ms
            .lock()
            .unwrap()
            .map(Duration::from_millis)
    }

    async fn apply_delay(spec: &LlmResponseSpec) {
        if let Some(ms) = spec.delay_ms() {
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
    }

    fn synthesize_from_stream(
        request: &LlmRequest,
        events: &[MessageStreamEvent],
    ) -> LlmResponseType {
        for event in events.iter().rev() {
            let (message, usage) = match event {
                MessageStreamEvent::FinalMessage(final_msg) => {
                    (final_msg.message.clone(), final_msg.usage.clone())
                }
                MessageStreamEvent::Message(msg) => (msg.message.clone(), None),
                _ => continue,
            };
            return LlmResponseType {
                id: None,
                model: request.profile_id.clone(),
                content: Some(extract_text_content(&message)),
                message,
                tool_calls: None,
                usage,
                finish_reason: Some("stop".to_string()),
                duration: 0,
                reasoning_content: None,
                reasoning_tokens: None,
                metadata: None,
                stream_stats: None,
                warnings: None,
            };
        }
        LlmResponseSpec::text("").build(request)
    }
}

impl Default for MockLlmClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmClient for MockLlmClient {
    async fn generate(&self, request: &LlmRequest) -> LlmResult<LlmResponseType> {
        self.record(request);
        match self.pop() {
            Some(ScriptedResponse::Spec(spec)) => {
                Self::apply_delay(&spec).await;
                Ok(spec.build(request))
            }
            Some(ScriptedResponse::Error(error)) => Err(error),
            Some(ScriptedResponse::Stream(events)) => {
                Ok(Self::synthesize_from_stream(request, &events))
            }
            None => {
                let spec = self.fallback_spec();
                Self::apply_delay(&spec).await;
                Ok(spec.build(request))
            }
        }
    }

    async fn generate_stream(&self, request: &LlmRequest) -> LlmResult<Box<dyn MessageStream>> {
        self.record(request);
        let events = match self.pop() {
            Some(ScriptedResponse::Stream(events)) => events,
            Some(ScriptedResponse::Spec(spec)) => spec.to_stream_events(request),
            Some(ScriptedResponse::Error(error)) => return Err(error),
            None => self.fallback_spec().to_stream_events(request),
        };
        Ok(Box::new(MockMessageStream::new(
            events,
            self.stream_delay(),
        )))
    }

    async fn count_tokens(&self, request: &LlmRequest) -> LlmResult<u32> {
        Ok(crate::client::estimate_request_tokens(request))
    }
}

/// Stream that replays a scripted event list (chunk -> final message -> end).
pub struct MockMessageStream {
    events: Vec<MessageStreamEvent>,
    index: usize,
    delay: Option<Duration>,
}

impl MockMessageStream {
    pub fn new(events: Vec<MessageStreamEvent>, delay: Option<Duration>) -> Self {
        Self {
            events,
            index: 0,
            delay,
        }
    }
}

#[async_trait::async_trait]
impl MessageStream for MockMessageStream {
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>> {
        if self.index >= self.events.len() {
            return None;
        }
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        let event = self.events[self.index].clone();
        self.index += 1;
        Some(Ok(event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::MessageStreamChunk;
    use wf_types::message::{LlmFunctionCall, MessageRole};

    fn request(profile_id: &str, text: &str) -> LlmRequest {
        LlmRequest {
            profile_id: profile_id.to_string(),
            messages: vec![Message {
                id: wf_types::Id::new(),
                role: MessageRole::User,
                content: MessageContentValue::Text(text.to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
            parameters: None,
            tools: None,
            tool_call_format: None,
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
        }
    }

    #[tokio::test]
    async fn scripted_text_is_served_in_order() {
        let client = MockLlmClient::new();
        client.script(LlmResponseSpec::text("first"));
        client.script(LlmResponseSpec::text("second"));

        let r1 = client.generate(&request("mock", "hi")).await.unwrap();
        let r2 = client.generate(&request("mock", "hi")).await.unwrap();
        assert_eq!(r1.content.as_deref(), Some("first"));
        assert_eq!(r2.content.as_deref(), Some("second"));
        assert_eq!(client.recorded_count(), 2);
        assert_eq!(
            client.recorded_requests()[0].messages[0].content,
            MessageContentValue::Text("hi".to_string())
        );
    }

    #[tokio::test]
    async fn falls_back_to_default_when_script_exhausted() {
        let client = MockLlmClient::new();
        client.default(LlmResponseSpec::text("fallback"));
        let result = client.generate(&request("mock", "hi")).await.unwrap();
        assert_eq!(result.content.as_deref(), Some("fallback"));
    }

    #[tokio::test]
    async fn scripted_error_is_returned() {
        let client = MockLlmClient::new();
        client.script_error(LlmError::ProviderError("HTTP 500 boom".to_string()));
        let err = client.generate(&request("mock", "hi")).await.unwrap_err();
        assert!(err.to_string().contains("boom"));
    }

    #[tokio::test]
    async fn tool_calls_spec_builds_matching_message() {
        let client = MockLlmClient::new();
        client.script(LlmResponseSpec::tool_calls(vec![LlmToolCall {
            id: "call_1".to_string(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: "search".to_string(),
                arguments: r#"{"q":"rust"}"#.to_string(),
            },
        }]));
        let result = client.generate(&request("mock", "hi")).await.unwrap();
        assert_eq!(result.tool_calls.as_ref().unwrap().len(), 1);
        assert_eq!(result.message.role, MessageRole::Assistant);
        assert_eq!(result.message.tool_calls.as_ref().unwrap()[0].id, "call_1");
    }

    #[tokio::test]
    async fn usage_and_reasoning_are_passed_through() {
        let client = MockLlmClient::new();
        client.script(
            LlmResponseSpec::text("thinking hard")
                .with_usage(10, 20)
                .with_reasoning("chain of thought")
                .with_model("mock-model")
                .with_finish_reason("stop"),
        );
        let result = client.generate(&request("mock", "hi")).await.unwrap();
        assert_eq!(result.usage.as_ref().unwrap().total_tokens, 30);
        assert_eq!(
            result.reasoning_content.as_deref(),
            Some("chain of thought")
        );
        assert_eq!(result.model, "mock-model");
        assert_eq!(result.finish_reason.as_deref(), Some("stop"));
    }

    #[tokio::test]
    async fn stream_events_are_replayed_in_order() {
        let client = MockLlmClient::new();
        let assistant = Message {
            id: wf_types::Id::new(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text("hello world".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        client.script_stream(vec![
            MessageStreamEvent::Stream(MessageStreamChunk {
                content: "hello ".to_string(),
            }),
            MessageStreamEvent::Stream(MessageStreamChunk {
                content: "world".to_string(),
            }),
            MessageStreamEvent::FinalMessage(MessageStreamFinal {
                message: assistant,
                usage: None,
            }),
            MessageStreamEvent::End(MessageStreamEnd {}),
        ]);

        let mut stream = client
            .generate_stream(&request("mock", "hi"))
            .await
            .unwrap();
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], MessageStreamEvent::Stream(_)));
        assert!(matches!(events[3], MessageStreamEvent::End(_)));
    }

    #[tokio::test]
    async fn generate_on_stream_script_synthesizes_final_result() {
        let client = MockLlmClient::new();
        let assistant = Message {
            id: wf_types::Id::new(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text("aggregated".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        client.script_stream(vec![
            MessageStreamEvent::FinalMessage(MessageStreamFinal {
                message: assistant,
                usage: None,
            }),
            MessageStreamEvent::End(MessageStreamEnd {}),
        ]);
        let result = client.generate(&request("mock", "hi")).await.unwrap();
        assert_eq!(result.content.as_deref(), Some("aggregated"));
    }

    #[tokio::test]
    async fn count_tokens_matches_production_estimation() {
        let client = MockLlmClient::new();
        let req = request("mock", "a b c d e f g h"); // 15 chars -> 4 tokens
        assert_eq!(client.count_tokens(&req).await.unwrap(), 4);
    }
}

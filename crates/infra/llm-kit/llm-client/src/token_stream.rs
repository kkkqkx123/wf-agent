use std::sync::Arc;

use wf_types::llm::{MessageStreamEvent, StreamStats, TokenUsageStats};

use llm_codec::error::LlmError;
use crate::stream::MessageStream;

/// Generic token-usage sink decoupled from any concrete metrics collector.
/// Implementations receive prompt/completion token counts, cost and model
/// name; the facade layer wires a project-specific adapter (e.g. the
/// metrics crate collector) into this trait.
pub trait TokenUsageSink: Send + Sync {
    fn record_token_usage(
        &self,
        prompt_tokens: u64,
        completion_tokens: u64,
        total_cost: Option<f64>,
        model: Option<&str>,
    );
}

/// Observation points for gateway request metrics, decoupled from any
/// concrete metrics collector. The facade layer wires a project-specific
/// adapter into this trait; the transport and gateway layers stay leaf.
pub trait LlmMetricsSink: TokenUsageSink {
    fn record_first_byte(&self, duration_ms: f64, model: Option<&str>);
    fn record_request(
        &self,
        duration_ms: f64,
        success: bool,
        error_kind: Option<&str>,
        model: Option<&str>,
    );
    fn record_retry(&self, model: Option<&str>);
}

/// Convenience alias for shared token-usage sinks.
pub type SharedTokenUsageSink = Arc<dyn TokenUsageSink>;

/// Convenience alias for shared gateway metrics sinks.
pub type SharedLlmMetricsSink = Arc<dyn LlmMetricsSink>;

/// Stream wrapper that records token usage from mid-stream usage events
/// (OpenAI `include_usage` chunk, Anthropic `message_delta`) or, as a
/// fallback, from the final message once the stream is exhausted. It also
/// collects stream statistics (chunk count / first-chunk latency / stream
/// and total durations) and attaches them to the `FinalMessage` event.
pub struct TokenRecordingStream {
    inner: Box<dyn MessageStream>,
    sink: Option<SharedTokenUsageSink>,
    model: String,
    last_usage: Option<TokenUsageStats>,
    recorded: bool,
    start_time: i64,
    first_chunk_time: Option<i64>,
    last_chunk_time: Option<i64>,
    chunk_count: u32,
}

impl TokenRecordingStream {
    pub fn new(
        inner: Box<dyn MessageStream>,
        sink: Option<SharedTokenUsageSink>,
        model: String,
    ) -> Self {
        Self {
            inner,
            sink,
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
        let Some(sink) = &self.sink else {
            return;
        };
        self.recorded = true;
        sink.record_token_usage(
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
    use wf_types::message::{Message, MessageContentValue, MessageRole};

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
        let message = Message {
            id: wf_types::Id::new(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text("hello world".to_string()),
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
}

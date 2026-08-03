use crate::dead_loop_detector::DeadLoopDetector;
use crate::error::LlmError;
use crate::formatters::LlmFormatter;
use async_trait::async_trait;
use eventsource_stream::EventStream;
use futures::StreamExt;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use wf_types::llm::{
    MessageStreamEvent, MessageStreamFinal, MessageStreamInputJson, MessageStreamReasoning,
    MessageStreamText, TokenUsageStats,
};
use wf_types::message::{
    LlmFunctionCall, LlmToolCall, Message, MessageContent, MessageContentValue, MessageRole,
};

#[async_trait]
pub trait MessageStream: Send {
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>>;
}

pub struct SseMessageStream<S> {
    stream: EventStream<S>,
    formatter: Arc<dyn LlmFormatter>,
    cancel: Option<CancellationToken>,
    done: bool,
    /// The formatter emitted `End`; the accumulator built the `FinalMessage`
    /// and the `End` event itself is pending on the next `next()` call.
    pending_end: bool,
    accumulator: MessageAccumulator,
}

impl<S> SseMessageStream<S> {
    pub fn new(
        stream: EventStream<S>,
        formatter: Arc<dyn LlmFormatter>,
        cancel: Option<CancellationToken>,
        dead_loop_config: Option<&wf_types::llm::DeadLoopDetectionConfig>,
    ) -> Self {
        Self {
            stream,
            formatter,
            cancel,
            done: false,
            pending_end: false,
            accumulator: MessageAccumulator::new(dead_loop_config),
        }
    }
}

#[async_trait]
impl<S, B, E> MessageStream for SseMessageStream<S>
where
    S: futures::Stream<Item = Result<B, E>> + Unpin + Send + 'static,
    B: AsRef<[u8]>,
    E: std::fmt::Display + Send + 'static,
{
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>> {
        if self.done {
            return None;
        }

        // The final message was already delivered; terminate with the
        // pending `End` event before the stream is exhausted.
        if self.pending_end {
            self.pending_end = false;
            self.done = true;
            return Some(Ok(MessageStreamEvent::End(
                wf_types::llm::MessageStreamEnd {},
            )));
        }

        if let Some(ref cancel) = self.cancel {
            if cancel.is_cancelled() {
                self.done = true;
                return Some(Err(LlmError::Cancelled));
            }
        }

        loop {
            match self.stream.next().await {
                Some(Ok(event)) => {
                    let data = event.data.trim().to_string();

                    if data.is_empty() {
                        continue;
                    }

                    match self.formatter.parse_stream_chunk(&data) {
                        Ok(Some(raw_event)) => {
                            // Always route `End` through the accumulator: it
                            // assembles the `FinalMessage` (content, tool calls,
                            // usage) before the stream terminates. The `End`
                            // event itself is emitted on the next call.
                            if let Some(emitted) = self.accumulator.push(raw_event) {
                                match emitted {
                                    MessageStreamEvent::Abort(_) => {
                                        self.done = true;
                                    }
                                    MessageStreamEvent::End(_) => {
                                        self.done = true;
                                    }
                                    MessageStreamEvent::FinalMessage(_) => {
                                        self.pending_end = true;
                                    }
                                    _ => {}
                                }
                                return Some(Ok(emitted));
                            }
                            continue;
                        }
                        Ok(None) => continue,
                        Err(e) => return Some(Err(e)),
                    }
                }
                Some(Err(e)) => {
                    self.done = true;
                    return Some(Err(LlmError::StreamError(e.to_string())));
                }
                None => {
                    self.done = true;
                    return None;
                }
            }
        }
    }
}

/// Accumulates streaming events into message snapshots.
pub struct MessageAccumulator {
    text_snapshot: String,
    reasoning_snapshot: String,
    tool_calls: BTreeMap<usize, PartialToolCall>,
    content_blocks: Vec<MessageContent>,
    usage: Option<TokenUsageStats>,
    dead_loop_detector: Option<DeadLoopDetector>,
    final_emitted: bool,
}

#[derive(Debug, Clone)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    index: usize,
}

impl MessageAccumulator {
    pub fn new(dead_loop_config: Option<&wf_types::llm::DeadLoopDetectionConfig>) -> Self {
        let dead_loop_detector = dead_loop_config.map(|c| DeadLoopDetector::new(c.into()));
        Self {
            text_snapshot: String::new(),
            reasoning_snapshot: String::new(),
            tool_calls: BTreeMap::new(),
            content_blocks: Vec::new(),
            usage: None,
            dead_loop_detector,
            final_emitted: false,
        }
    }

    /// Push a raw event and return the semantically-enriched event to emit.
    pub fn push(&mut self, event: MessageStreamEvent) -> Option<MessageStreamEvent> {
        match event {
            MessageStreamEvent::Text(text_event) => {
                self.text_snapshot.push_str(&text_event.text);
                Some(MessageStreamEvent::Text(MessageStreamText {
                    text: text_event.text,
                    snapshot: self.text_snapshot.clone(),
                }))
            }
            MessageStreamEvent::ReasoningText(reasoning_event) => {
                self.reasoning_snapshot.push_str(&reasoning_event.reasoning);

                // Dead loop detection
                if let Some(ref mut detector) = self.dead_loop_detector {
                    let result = detector.detect(&self.reasoning_snapshot);
                    if result.detected {
                        return Some(MessageStreamEvent::Abort(
                            wf_types::llm::MessageStreamAbort {
                                reason: result
                                    .details
                                    .unwrap_or_else(|| "Dead loop detected".to_string()),
                            },
                        ));
                    }
                }

                Some(MessageStreamEvent::ReasoningText(MessageStreamReasoning {
                    reasoning: reasoning_event.reasoning,
                    snapshot: self.reasoning_snapshot.clone(),
                }))
            }
            MessageStreamEvent::InputJson(input_json) => {
                // For Anthropic-style input_json_delta, accumulate by index
                let index = input_json.index.unwrap_or(0);
                let partial_tool_call =
                    self.tool_calls
                        .entry(index)
                        .or_insert_with(|| PartialToolCall {
                            id: None,
                            name: None,
                            arguments: String::new(),
                            index,
                        });
                partial_tool_call
                    .arguments
                    .push_str(&input_json.partial_json);

                // Try to parse the accumulated JSON
                let parsed_snapshot =
                    crate::partial_json_parser::parse_partial_json(&partial_tool_call.arguments)
                        .as_complete()
                        .cloned();

                Some(MessageStreamEvent::InputJson(MessageStreamInputJson {
                    partial_json: input_json.partial_json,
                    index: Some(index),
                    parsed_snapshot,
                }))
            }
            MessageStreamEvent::ToolCallDelta(delta) => {
                // Accumulate tool call fragments
                let partial =
                    self.tool_calls
                        .entry(delta.index)
                        .or_insert_with(|| PartialToolCall {
                            id: None,
                            name: None,
                            arguments: String::new(),
                            index: delta.index,
                        });

                if delta.is_snapshot {
                    // Complete snapshot (Gemini functionCall, OpenAI response
                    // function_call.done): replace accumulated fragments.
                    if let Some(id) = delta.id.clone() {
                        partial.id = Some(id);
                    }
                    if let Some(name) = delta.name.clone() {
                        partial.name = Some(name);
                    }
                    if let Some(ref args) = delta.arguments {
                        partial.arguments = args.clone();
                    }
                } else {
                    if let Some(id) = delta.id.clone() {
                        partial.id = Some(id);
                    }
                    if let Some(name) = delta.name.clone() {
                        partial.name = Some(name);
                    }
                    if let Some(ref args) = delta.arguments {
                        partial.arguments.push_str(args);
                    }
                }

                // Return the delta for consumers that want incremental updates
                Some(MessageStreamEvent::ToolCallDelta(delta))
            }
            MessageStreamEvent::Usage(usage_event) => {
                // Merge usage (later values win for non-zero fields)
                let u = &usage_event.usage;
                if u.prompt_tokens > 0 || u.completion_tokens > 0 || u.total_tokens > 0 {
                    self.usage = Some(u.clone());
                }
                Some(MessageStreamEvent::Usage(usage_event))
            }
            MessageStreamEvent::End(_) => {
                if self.final_emitted {
                    return None;
                }
                self.final_emitted = true;

                // Build final message
                let tool_calls = self.assemble_tool_calls();
                let content = if self.content_blocks.is_empty() {
                    MessageContentValue::Text(self.text_snapshot.clone())
                } else {
                    MessageContentValue::Rich(self.content_blocks.clone())
                };

                let message = Message {
                    id: wf_types::Id::new(),
                    role: MessageRole::Assistant,
                    content,
                    timestamp: wf_common::time::now(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    thinking: if self.reasoning_snapshot.is_empty() {
                        None
                    } else {
                        Some(self.reasoning_snapshot.clone())
                    },
                    metadata: None,
                };

                Some(MessageStreamEvent::FinalMessage(MessageStreamFinal {
                    message,
                    usage: self.usage.clone(),
                    stream_stats: None, // Filled by the gateway stream wrapper
                }))
            }
            // Pass through other events unchanged
            _ => Some(event),
        }
    }

    fn assemble_tool_calls(&self) -> Vec<LlmToolCall> {
        self.tool_calls
            .values()
            .filter_map(|partial| {
                let name = partial.name.clone()?;
                let id = partial.id.clone().unwrap_or_else(|| {
                    format!("call_{}_{}", wf_common::time::now(), partial.index)
                });
                let arguments = if partial.arguments.is_empty() {
                    "{}".to_string()
                } else {
                    partial.arguments.clone()
                };
                Some(LlmToolCall {
                    id,
                    r#type: "function".to_string(),
                    function: LlmFunctionCall { name, arguments },
                })
            })
            .collect()
    }
}

impl PartialToolCall {}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::{MessageStreamToolCallDelta, MessageStreamUsage};

    /// Feed a real OpenAI-style SSE byte stream through `SseMessageStream` with
    /// the real `OpenaiChatFormatter`, proving that the `FinalMessage` is
    /// assembled and delivered before the `End` event (regression test for the
    /// `End` short-circuit that skipped the accumulator).
    #[tokio::test]
    async fn real_sse_path_emits_final_message_before_end() {
        use futures::stream;
        use wf_types::llm::MessageStreamEvent;

        let sse = concat!(
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"}}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"Bei\"}}]}}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"jing\\\"}\"}}]}}]}\n\n",
            "data: {\"id\":\"1\",\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
            "data: [DONE]\n\n",
        );

        let inner = stream::iter(vec![Ok::<_, std::io::Error>(sse.as_bytes().to_vec())]);
        let sse_stream = EventStream::new(inner);
        let formatter: Arc<dyn LlmFormatter> = Arc::new(crate::formatters::OpenaiChatFormatter::new());
        let mut stream = SseMessageStream::new(sse_stream, formatter, None, None);

        let mut events: Vec<MessageStreamEvent> = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.expect("no stream error"));
        }

        let text: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                MessageStreamEvent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(text, vec!["Hello", " world"]);

        let final_msg = events
            .iter()
            .find_map(|e| match e {
                MessageStreamEvent::FinalMessage(f) => Some(f),
                _ => None,
            })
            .expect("FinalMessage must be emitted");
        assert_eq!(
            crate::message_helper::extract_text_content(&final_msg.message),
            "Hello world"
        );
        let calls = final_msg
            .message
            .tool_calls
            .as_ref()
            .expect("tool calls must be assembled");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "get_weather");
        assert_eq!(calls[0].function.arguments, r#"{"city":"Beijing"}"#);
        assert_eq!(final_msg.usage.as_ref().unwrap().total_tokens, 15);

        let final_index = events
            .iter()
            .position(|e| matches!(e, MessageStreamEvent::FinalMessage(_)))
            .unwrap();
        let end_index = events
            .iter()
            .position(|e| matches!(e, MessageStreamEvent::End(_)))
            .unwrap();
        assert!(final_index < end_index, "FinalMessage must precede End");
        assert!(matches!(events.last(), Some(MessageStreamEvent::End(_))));
    }

    #[test]
    fn openai_style_deltas_accumulate_into_full_tool_call() {
        let mut acc = MessageAccumulator::new(None);

        acc.push(MessageStreamEvent::ToolCallDelta(
            MessageStreamToolCallDelta {
                index: 0,
                id: Some("call_1".to_string()),
                name: Some("get_weather".to_string()),
                arguments: Some(r#"{"city":"Bei"#.to_string()),
                is_snapshot: false,
            },
        ));
        acc.push(MessageStreamEvent::ToolCallDelta(
            MessageStreamToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments: Some(r#"jing"}"#.to_string()),
                is_snapshot: false,
            },
        ));
        acc.push(MessageStreamEvent::ToolCallDelta(
            MessageStreamToolCallDelta {
                index: 1,
                id: Some("call_2".to_string()),
                name: Some("get_time".to_string()),
                arguments: Some("{}".to_string()),
                is_snapshot: false,
            },
        ));

        let final_event = acc.push(MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}));
        let MessageStreamEvent::FinalMessage(final_msg) = final_event.unwrap() else {
            panic!("expected FinalMessage");
        };
        let calls = final_msg.message.tool_calls.unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "get_weather");
        assert_eq!(calls[0].function.arguments, r#"{"city":"Beijing"}"#);
        assert_eq!(calls[1].function.name, "get_time");
    }

    #[test]
    fn snapshot_delta_replaces_fragments() {
        let mut acc = MessageAccumulator::new(None);

        acc.push(MessageStreamEvent::ToolCallDelta(
            MessageStreamToolCallDelta {
                index: 0,
                id: Some("fc_1".to_string()),
                name: None,
                arguments: Some(r#"{"a":"#.to_string()),
                is_snapshot: false,
            },
        ));
        acc.push(MessageStreamEvent::ToolCallDelta(
            MessageStreamToolCallDelta {
                index: 0,
                id: Some("fc_1".to_string()),
                name: Some("get_weather".to_string()),
                arguments: Some(r#"{"city":"Beijing"}"#.to_string()),
                is_snapshot: true,
            },
        ));

        let final_event = acc.push(MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}));
        let MessageStreamEvent::FinalMessage(final_msg) = final_event.unwrap() else {
            panic!("expected FinalMessage");
        };
        let calls = final_msg.message.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "get_weather");
        assert_eq!(calls[0].function.arguments, r#"{"city":"Beijing"}"#);
    }

    #[test]
    fn text_snapshot_and_usage_merge_on_final_message() {
        let mut acc = MessageAccumulator::new(None);

        acc.push(MessageStreamEvent::Text(MessageStreamText {
            text: "hello".to_string(),
            snapshot: String::new(),
        }));
        acc.push(MessageStreamEvent::Text(MessageStreamText {
            text: " world".to_string(),
            snapshot: String::new(),
        }));
        acc.push(MessageStreamEvent::Usage(MessageStreamUsage {
            usage: TokenUsageStats {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                prompt_tokens_cost: None,
                completion_tokens_cost: None,
                total_cost: None,
            },
        }));

        let final_event = acc.push(MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {}));
        let MessageStreamEvent::FinalMessage(final_msg) = final_event.unwrap() else {
            panic!("expected FinalMessage");
        };
        assert_eq!(
            final_msg.message.content,
            MessageContentValue::Text("hello world".to_string())
        );
        let usage = final_msg.usage.unwrap();
        assert_eq!(usage.total_tokens, 15);
    }
}

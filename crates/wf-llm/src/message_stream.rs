use async_trait::async_trait;
use crate::error::LlmError;
use wf_types::llm::MessageStreamEvent;
use eventsource_stream::EventStream;
use futures::StreamExt;

#[async_trait]
pub trait MessageStream: Send {
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>>;
}

pub struct SseMessageStream<S> {
    stream: EventStream<S>,
}

impl<S> SseMessageStream<S> {
    pub fn new(stream: EventStream<S>) -> Self {
        Self { stream }
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
        match self.stream.next().await {
            Some(Ok(event)) => {
                if event.data == "[DONE]" {
                    return Some(Ok(MessageStreamEvent::End(
                        wf_types::llm::MessageStreamEnd {},
                    )));
                }

                match serde_json::from_str::<serde_json::Value>(&event.data) {
                    Ok(json) => {
                        if let Some(choices) = json.get("choices").and_then(|v| v.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                        return Some(Ok(MessageStreamEvent::Text(
                                            wf_types::llm::MessageStreamText {
                                                text: content.to_string(),
                                            },
                                        )));
                                    }
                                }
                            }
                        }
                        Some(Ok(MessageStreamEvent::Stream(
                            wf_types::llm::MessageStreamChunk {
                                content: event.data,
                            },
                        )))
                    }
                    Err(e) => Some(Err(LlmError::SerializationError(e))),
                }
            }
            Some(Err(e)) => Some(Err(LlmError::StreamError(e.to_string()))),
            None => None,
        }
    }
}

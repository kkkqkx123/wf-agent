use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use crate::error::LlmError;
use crate::formatters::LlmFormatter;
use wf_types::llm::MessageStreamEvent;
use eventsource_stream::EventStream;
use futures::StreamExt;

#[async_trait]
pub trait MessageStream: Send {
    async fn next(&mut self) -> Option<Result<MessageStreamEvent, LlmError>>;
}

pub struct SseMessageStream<S> {
    stream: EventStream<S>,
    formatter: Arc<dyn LlmFormatter>,
    cancel: Option<CancellationToken>,
    done: bool,
}

impl<S> SseMessageStream<S> {
    pub fn new(stream: EventStream<S>, formatter: Arc<dyn LlmFormatter>, cancel: Option<CancellationToken>) -> Self {
        Self { stream, formatter, cancel, done: false }
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
                        Ok(Some(MessageStreamEvent::End(_))) => {
                            self.done = true;
                            return Some(Ok(MessageStreamEvent::End(wf_types::llm::MessageStreamEnd {})));
                        }
                        Ok(Some(event)) => return Some(Ok(event)),
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

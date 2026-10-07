//! Server-sent-events streaming for chat completions.
//!
//! Reads `POST {base_url}/chat/completions` with `"stream": true` through
//! `eventsource-stream` and surfaces content deltas. Multi-protocol codec
//! streaming stays in `wf-llm`; this module only covers the plain
//! OpenAI-compatible delta shape.

use std::pin::Pin;

use eventsource_stream::{Event, EventStream, EventStreamError};
use futures::StreamExt;
use serde_json::Value;

use crate::error::{ChatError, Result};

/// One event from a streaming chat response.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatStreamEvent {
    /// Incremental content fragment.
    Delta(String),
    /// Provider-declared finish reason (e.g. `stop`, `length`).
    Finished(String),
}

/// SSE stream of [`ChatStreamEvent`] backed by a chat HTTP response.
///
/// The inner byte stream is boxed so callers name one concrete type.
pub struct ChatEventStream {
    inner: Pin<Box<dyn futures::Stream<Item = EventStreamItem> + Send>>,
    done: bool,
}

type EventStreamItem = std::result::Result<Event, EventStreamError<reqwest::Error>>;

impl ChatEventStream {
    /// Wraps a streaming chat HTTP response.
    pub fn new(response: reqwest::Response) -> Self {
        let stream = EventStream::new(response.bytes_stream());
        Self {
            inner: Box::pin(stream),
            done: false,
        }
    }

    /// Returns the next stream event, or `None` when the stream ended.
    pub async fn next(&mut self) -> Option<Result<ChatStreamEvent>> {
        if self.done {
            return None;
        }
        loop {
            match self.inner.next().await {
                None => {
                    self.done = true;
                    return None;
                }
                Some(Err(err)) => {
                    self.done = true;
                    return Some(Err(ChatError::Transport(err.to_string())));
                }
                Some(Ok(event)) => {
                    let data = event.data.trim();
                    if data.is_empty() {
                        continue;
                    }
                    if data == "[DONE]" {
                        self.done = true;
                        return None;
                    }
                    match parse_stream_chunk(data) {
                        Ok(None) => continue,
                        Ok(Some(parsed)) => return Some(Ok(parsed)),
                        Err(err) => {
                            self.done = true;
                            return Some(Err(err));
                        }
                    }
                }
            }
        }
    }
}

/// Parses one SSE data payload into a stream event.
///
/// Returns `Ok(None)` for payloads without usable content (e.g. role-only
/// first chunks), so the caller keeps draining the stream.
pub fn parse_stream_chunk(data: &str) -> Result<Option<ChatStreamEvent>> {
    let value: Value =
        serde_json::from_str(data).map_err(|err| ChatError::Decode(err.to_string()))?;
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first());

    if let Some(reason) = choice
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(Value::as_str)
    {
        return Ok(Some(ChatStreamEvent::Finished(reason.to_string())));
    }

    let delta = choice
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if delta.is_empty() {
        return Ok(None);
    }
    Ok(Some(ChatStreamEvent::Delta(delta.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_delta() {
        let data = r#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        assert_eq!(
            parse_stream_chunk(data).expect("parsed"),
            Some(ChatStreamEvent::Delta("Hello".into()))
        );
    }

    #[test]
    fn skips_role_only_chunks() {
        let data = r#"{"choices":[{"delta":{"role":"assistant"},"finish_reason":null}]}"#;
        assert_eq!(parse_stream_chunk(data).expect("parsed"), None);
    }

    #[test]
    fn surfaces_finish_reason() {
        let data = r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        assert_eq!(
            parse_stream_chunk(data).expect("parsed"),
            Some(ChatStreamEvent::Finished("stop".into()))
        );
    }

    #[test]
    fn rejects_malformed_payloads() {
        assert!(parse_stream_chunk("not json").is_err());
    }
}

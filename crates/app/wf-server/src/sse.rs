//! Shared SSE response construction: status line and headers for a
//! text/event-stream body, plus the frame writer for the execution protocol.
//! Callers build the frame stream (e.g. event subscription or execution stream
//! frames) and hand it to `sse_response`.

use std::convert::Infallible;

use axum::body::{Body, Bytes};
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use futures::StreamExt;
use wf_api::infra::stream::ExecutionStreamEvent;

/// Write execution-protocol frames as SSE data lines.
///
/// An engine event carries its own `type` discriminator, so it is named on the
/// `event:` line and written as the bare bus event: internally tagging it under
/// the protocol `type` would put two `type` keys into one JSON object.
pub(crate) fn execution_frames<S>(
    stream: S,
) -> impl futures::Stream<Item = Result<Bytes, Infallible>> + Send
where
    S: futures::Stream<Item = ExecutionStreamEvent> + Send,
{
    stream.map(|event| {
        let encoded = match &event {
            ExecutionStreamEvent::Engine(base) => serde_json::to_string(base)
                .map(|payload| format!("event: engine\ndata: {payload}\n\n")),
            other => serde_json::to_string(other).map(|payload| format!("data: {payload}\n\n")),
        };
        match encoded {
            Ok(frame) => Ok(Bytes::from(frame)),
            Err(err) => {
                tracing::error!(%err, "execution stream frame dropped");
                Ok(Bytes::new())
            }
        }
    })
}

pub(crate) fn sse_response<S>(stream: S) -> Response
where
    S: futures::Stream<Item = Result<axum::body::Bytes, Infallible>> + Send + 'static,
{
    let response = Response::new(Body::from_stream(stream));
    let (mut parts, body) = response.into_parts();
    parts.status = StatusCode::OK;
    parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    parts
        .headers
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Response::from_parts(parts, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Bytes};
    use futures::StreamExt;
    use serde_json::Value;
    use wf_types::events::{BaseEvent, EventType};
    use wf_types::Id;

    #[tokio::test]
    async fn sse_response_sets_stream_headers() {
        let response = sse_response(futures::stream::empty::<Result<Bytes, Infallible>>());
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .expect("content type"),
            "text/event-stream"
        );
        assert_eq!(
            response
                .headers()
                .get(CACHE_CONTROL)
                .expect("cache control"),
            "no-cache"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("empty body");
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn execution_frames_distinguish_engine_events_from_protocol_events() {
        let event = BaseEvent {
            id: Id::from("event-1".to_string()),
            r#type: EventType::WorkflowExecutionStarted,
            timestamp: 42,
            event_name: None,
            workflow_id: None,
            execution_id: Some(Id::from("exec-1".to_string())),
            agent_loop_id: None,
            metadata: None,
        };
        let events = vec![
            ExecutionStreamEvent::Engine(event),
            ExecutionStreamEvent::LlmDelta {
                content: "hello".to_string(),
            },
        ];
        let mut frames = execution_frames(futures::stream::iter(events));

        let engine = frames
            .next()
            .await
            .expect("engine frame")
            .expect("engine bytes");
        let engine = String::from_utf8(engine.to_vec()).expect("engine text");
        let payload = engine
            .strip_prefix("event: engine\ndata: ")
            .expect("engine label")
            .strip_suffix("\n\n")
            .expect("frame terminator");
        let payload: Value = serde_json::from_str(payload).expect("engine payload");
        assert_eq!(payload["type"], "WORKFLOW_EXECUTION_STARTED");
        assert_eq!(payload["id"], "event-1");

        let delta = frames
            .next()
            .await
            .expect("delta frame")
            .expect("delta bytes");
        assert_eq!(
            String::from_utf8(delta.to_vec()).expect("delta text"),
            "data: {\"type\":\"llm_delta\",\"content\":\"hello\"}\n\n"
        );
        assert!(frames.next().await.is_none());
    }
}

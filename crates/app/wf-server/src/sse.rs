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

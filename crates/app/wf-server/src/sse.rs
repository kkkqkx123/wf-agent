//! Shared SSE response construction: status line and headers for a
//! text/event-stream body. Callers build the frame stream (e.g. event
//! subscription or execution stream frames) and hand it to `sse_response`.

use std::convert::Infallible;

use axum::body::Body;
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;

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

//! Authentication and single-use ticket integration at the router boundary.
//! The tests use the real middleware stack and stream handlers without
//! binding a TCP port or consuming an infinite SSE body.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use wf_api::ApiContext;
use wf_resource::registry::ResourceRegistries;
use wf_server::middleware::{AuthConfig, ServerMiddlewareConfig};
use wf_storage::context::StorageContext;

fn app() -> Router {
    let ctx = Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ));
    let config = ServerMiddlewareConfig {
        auth: AuthConfig {
            enabled: true,
            api_keys: vec!["test-key".to_string()],
            ..AuthConfig::default()
        },
        ..ServerMiddlewareConfig::default()
    };
    wf_server::api_router_with_config(ctx, Arc::new(config))
}

async fn json_call(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let response = raw_call(app, method, uri, headers, body).await;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn raw_call(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Response {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let payload = body.map(|value| value.to_string()).unwrap_or_default();
    app.clone()
        .oneshot(builder.body(Body::from(payload)).expect("request"))
        .await
        .expect("router response")
}

#[tokio::test]
async fn auth_rejects_missing_and_invalid_keys() {
    let app = app();

    let (status, body) = json_call(&app, "GET", "/api/v1/workflows", &[], None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["success"], false);

    let (status, body) = json_call(
        &app,
        "GET",
        "/api/v1/workflows",
        &[("x-api-key", "wrong-key")],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["success"], false);

    let (status, _) = json_call(
        &app,
        "GET",
        "/api/v1/workflows",
        &[("x-api-key", "test-key")],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn ticket_minting_requires_marker_header() {
    let app = app();

    let (status, body) = json_call(
        &app,
        "POST",
        "/api/v1/events/stream-ticket",
        &[("x-api-key", "test-key")],
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "TICKET_INVALID");

    let (status, body) = json_call(
        &app,
        "POST",
        "/api/v1/events/stream-ticket",
        &[("x-api-key", "test-key"), ("x-wf-ticket", "1")],
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert!(body["data"]["ticket"]
        .as_str()
        .is_some_and(|t| !t.is_empty()));
}

#[tokio::test]
async fn stream_ticket_is_consumed_once() {
    let app = app();

    let (_, body) = json_call(
        &app,
        "POST",
        "/api/v1/events/stream-ticket",
        &[("x-api-key", "test-key"), ("x-wf-ticket", "1")],
        Some(json!({})),
    )
    .await;
    let ticket = body["data"]["ticket"]
        .as_str()
        .expect("issued ticket")
        .to_string();

    let stream_uri = format!("/api/v1/events/stream?ticket={ticket}");
    let stream = raw_call(&app, "GET", &stream_uri, &[], None).await;
    assert_eq!(stream.status(), StatusCode::OK);
    assert_eq!(
        stream
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .expect("content type"),
        "text/event-stream"
    );
    drop(stream);

    let (status, body) = json_call(&app, "GET", &stream_uri, &[], None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "TICKET_INVALID");
}

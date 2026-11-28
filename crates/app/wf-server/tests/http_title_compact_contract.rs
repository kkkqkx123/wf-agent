//! Router-level contract tests for session title generation and manual
//! compaction. Both handlers validate execution-body ownership before doing
//! work, so unknown identifiers deterministically return a not-found
//! envelope and missing required fields are rejected by the Json extractor.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use wf_api::ApiContext;
use wf_resource::registry::ResourceRegistries;
use wf_server::middleware::ServerMiddlewareConfig;
use wf_storage::context::StorageContext;

fn app() -> Router {
    let ctx = ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    );
    wf_server::api_router_with_config(Arc::new(ctx), Arc::new(ServerMiddlewareConfig::default()))
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let payload = body.map(|value| value.to_string()).unwrap_or_default();
    let request = builder
        .body(Body::from(payload))
        .expect("request should be valid");
    let response = app.clone().oneshot(request).await.expect("router response");
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

fn assert_not_found(status: StatusCode, body: &Value, label: &str) {
    assert_eq!(body["success"], false, "{label}: {body}");
    assert_eq!(body["error"]["code"], "NOT_FOUND", "{label}: {body}");
    assert_eq!(status, StatusCode::NOT_FOUND, "{label}: {body}");
}

#[tokio::test]
async fn title_routes_return_not_found_for_unknown_sessions() {
    let app = app();
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/chat/title",
        Some(json!({"session_id": "missing-session", "profile_id": "p1"})),
    )
    .await;
    assert_not_found(status, &body, "chat title");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/agent-loops/missing-loop/title",
        Some(json!({"profile_id": "p1"})),
    )
    .await;
    assert_not_found(status, &body, "loop title");
}

#[tokio::test]
async fn title_route_rejects_missing_required_body_fields() {
    let app = app();
    let (status, _) = call(&app, "POST", "/api/v1/chat/title", Some(json!({}))).await;
    // Axum's Json extractor rejects a body missing required fields with a
    // transport-level 422 before the handler can produce an envelope.
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn compact_route_returns_not_found_for_unknown_loop() {
    let app = app();
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/agent-loops/missing-loop/compact",
        Some(json!({"reason": "contract test"})),
    )
    .await;
    assert_not_found(status, &body, "compact");
}

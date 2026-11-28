//! Router-level contract tests for the file checkpoint approval and
//! provenance surface, which the TUI and web app both consume. A memory
//! context does not attach a file checkpoint manager, so every endpoint in
//! this domain deterministically maps to an internal-error envelope.

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

fn assert_disabled_manager(status: StatusCode, body: &Value, label: &str) {
    assert_eq!(body["success"], false, "{label}: {body}");
    assert_eq!(body["error"]["code"], "INTERNAL_ERROR", "{label}: {body}");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{label}: {body}");
}

#[tokio::test]
async fn file_approval_routes_report_a_disabled_manager() {
    let app = app();
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/file-checkpoint/approvals/pending",
        None,
    )
    .await;
    assert_disabled_manager(status, &body, "pending approvals");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/file-checkpoint/approvals/missing/approve",
        Some(json!({"feature": "main"})),
    )
    .await;
    assert_disabled_manager(status, &body, "approve changes");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/file-checkpoint/approvals/missing/reject",
        Some(json!({"reason": "contract test"})),
    )
    .await;
    assert_disabled_manager(status, &body, "reject changes");
}

#[tokio::test]
async fn file_provenance_read_routes_report_a_disabled_manager() {
    let app = app();
    for uri in [
        "/api/v1/file-checkpoint/partitions",
        "/api/v1/file-checkpoint/conflicts",
        "/api/v1/file-checkpoint/gc/stats",
        "/api/v1/file-checkpoint/index/status",
    ] {
        let (status, body) = call(&app, "GET", uri, None).await;
        assert_disabled_manager(status, &body, uri);
    }
}

//! Router-level contract tests for the Git read/write surface. Git is a
//! workspace-bound domain: without a directory scope the facade rejects the
//! request at the precondition check, so these cases assert the deterministic
//! missing-scope and parameter-validation branches through the real router.

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

fn assert_validation_error(status: StatusCode, body: &Value, label: &str) {
    assert_eq!(body["success"], false, "{label}: {body}");
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR", "{label}: {body}");
    assert_eq!(status, StatusCode::BAD_REQUEST, "{label}: {body}");
}

#[tokio::test]
async fn git_read_routes_require_a_request_directory() {
    let app = app();
    for uri in [
        "/api/v1/git/status",
        "/api/v1/git/meta",
        "/api/v1/git/files",
        "/api/v1/git/diff",
        "/api/v1/git/diff/staged",
        "/api/v1/git/branches",
        "/api/v1/git/commits",
        "/api/v1/git/fingerprint",
    ] {
        let (status, body) = call(&app, "GET", uri, None).await;
        assert_validation_error(status, &body, uri);
        assert!(
            body["error"]["message"]
                .as_str()
                .expect("error message")
                .contains("directory"),
            "{uri}: {body}"
        );
    }
}

#[tokio::test]
async fn git_diff_range_rejects_missing_required_refs() {
    let app = app();
    let (status, _) = call(&app, "GET", "/api/v1/git/diff/range", None).await;
    // Axum's Query extractor rejects malformed queries at the transport
    // layer with a plain-text 400, before the handler builds an envelope.
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn git_action_rejects_empty_and_unknown_actions() {
    let app = app();
    let cases = [
        json!({"action": ""}),
        json!({"action": "definitely-not-a-git-action"}),
    ];
    for body in cases {
        let (status, response) = call(&app, "POST", "/api/v1/git/action", Some(body.clone())).await;
        assert_validation_error(status, &response, "git action");
    }
}

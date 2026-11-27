//! Router-level contract tests for API domains that previously had no
//! transport coverage. Requests run through the real axum router with a
//! memory-backed context, so extractors, middleware layering and envelopes
//! are exercised together.

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
use wf_types::config::user_git::UserGitConfig;

fn app() -> Router {
    let ctx = ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    )
    .with_user_git(UserGitConfig {
        enabled: true,
        ..Default::default()
    });
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

#[tokio::test]
async fn unknown_entities_use_the_not_found_envelope() {
    let app = app();
    for uri in [
        "/api/v1/workflows/missing-workflow",
        "/api/v1/executions/missing-execution/state",
        "/api/v1/executions/missing-execution/graph",
    ] {
        let (status, body) = call(&app, "GET", uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body["success"], false, "{uri}");
        assert_eq!(body["error"]["code"], "NOT_FOUND", "{uri}");
    }
}

#[tokio::test]
async fn entity_variables_round_trip_and_delete() {
    let app = app();
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/variables",
        Some(json!({"name": "contract-var", "value": 7, "scope": "default"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["value"], 7);

    let (status, body) = call(&app, "GET", "/api/v1/variables/contract-var", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["name"], "contract-var");
    assert_eq!(body["data"]["value"], 7);

    let (status, _) = call(&app, "DELETE", "/api/v1/variables/contract-var", None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = call(&app, "GET", "/api/v1/variables/contract-var", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn template_library_is_reachable_with_empty_registries() {
    let app = app();
    for uri in [
        "/api/v1/templates/library",
        "/api/v1/templates/library/featured",
        "/api/v1/templates/library/popular",
    ] {
        let (status, body) = call(&app, "GET", uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(body["success"], true, "{uri}");
        assert!(body["data"].is_object() || body["data"].is_array(), "{uri}");
    }
}

#[tokio::test]
async fn conversation_session_lifecycle_is_reachable() {
    let app = app();
    let (status, body) = call(&app, "POST", "/api/v1/chat/sessions", Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    let session_id = body["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert!(!session_id.is_empty());

    let snapshot = format!("/api/v1/chat/sessions/{session_id}");
    let (status, body) = call(&app, "GET", &snapshot, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);

    let (status, _) = call(&app, "GET", &format!("{snapshot}/grafts"), None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = call(&app, "DELETE", &snapshot, None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = call(&app, "GET", &snapshot, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[cfg(feature = "backends")]
#[tokio::test]
async fn backend_management_is_disabled_without_a_manager() {
    let app = app();
    let (status, body) = call(&app, "GET", "/api/v1/system/backends", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "SERVICE_UNAVAILABLE");
}

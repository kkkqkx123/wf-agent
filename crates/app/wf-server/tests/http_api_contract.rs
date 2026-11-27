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

#[tokio::test]
async fn entity_tasks_round_trip_and_filter() {
    let app = app();
    let task = json!({
        "id": "contract-task",
        "task_type": "review",
        "status": "pending",
        "created_at": 1000,
        "updated_at": 1000
    });
    let (status, body) = call(&app, "POST", "/api/v1/tasks", Some(task.clone())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["data"], "contract-task");

    let (status, body) = call(&app, "GET", "/api/v1/tasks/contract-task", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["task_type"], "review");

    let (status, body) = call(&app, "GET", "/api/v1/tasks?status=pending", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["items"].as_array().expect("items").len(), 1);

    let (status, _) = call(&app, "DELETE", "/api/v1/tasks/contract-task", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(&app, "GET", "/api/v1/tasks/contract-task", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn workflow_node_config_and_layout_round_trip() {
    let app = app();
    let workflow = json!({
        "id": "wf-canvas",
        "name": "Canvas workflow",
        "version": "1.0.0",
        "nodes": [
            {"id": "start", "node_type": "START", "name": "start"},
            {
                "id": "n1",
                "node_type": "acme.step",
                "name": "inserted",
                "config": {"template_id": "nt-script"}
            },
            {"id": "end", "node_type": "END", "name": "end"}
        ],
        "edges": [
            {"id": "e1", "source_node_id": "start", "target_node_id": "n1", "type": "DEFAULT"},
            {"id": "e2", "source_node_id": "n1", "target_node_id": "end", "type": "DEFAULT"}
        ],
        "created_at": 1000,
        "updated_at": 1000
    });
    let (status, body) = call(&app, "POST", "/api/v1/workflows", Some(workflow.clone())).await;
    assert_eq!(status, StatusCode::OK, "create body: {body}");
    assert_eq!(body["success"], true);

    let (status, body) = call(&app, "POST", "/api/v1/workflows/validate", Some(workflow)).await;
    assert_eq!(status, StatusCode::OK, "validation body: {body}");
    assert_eq!(body["success"], true);

    let layout = json!({"positions": {"n1": {"x": 12, "y": 34}}});
    let (status, body) = call(
        &app,
        "PUT",
        "/api/v1/workflows/wf-canvas/layout",
        Some(layout),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["positions"]["n1"]["x"], 12);

    let (status, body) = call(&app, "GET", "/api/v1/workflows/wf-canvas/layout", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["positions"]["n1"]["y"], 34);

    let (status, body) = call(&app, "GET", "/api/v1/workflows/wf-canvas", None).await;
    assert_eq!(status, StatusCode::OK);
    let nodes = body["data"]["nodes"].as_array().expect("stored nodes");
    let inserted = nodes
        .iter()
        .find(|node| node["id"] == "n1")
        .expect("inserted node");
    assert_eq!(inserted["config"]["template_id"], "nt-script");

    let (status, _) = call(&app, "DELETE", "/api/v1/workflows/wf-canvas/layout", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(&app, "GET", "/api/v1/workflows/missing/layout", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn workflow_lock_conflict_keeps_the_first_owner() {
    let app = app();
    let alice = json!({"owner_id": "alice", "owner_name": "Alice"});
    let bob = json!({"owner_id": "bob", "owner_name": "Bob"});
    let bob_id = json!({"owner_id": "bob"});
    let alice_id = json!({"owner_id": "alice"});

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/workflows/wf-lock/lock/acquire",
        Some(alice),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["owner_id"], "alice");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/workflows/wf-lock/lock/acquire",
        Some(bob),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["owner_id"], "alice");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/workflows/wf-lock/lock/heartbeat",
        Some(bob_id.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["owner_id"], "alice");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/workflows/wf-lock/lock/release",
        Some(bob_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["owner_id"], "alice");

    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/workflows/wf-lock/lock/release",
        Some(alice_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"].is_null());
}

#[tokio::test]
async fn node_template_draft_lifecycle_reaches_a_formal_template() {
    let app = app();
    let draft = json!({
        "id": "nt-draft",
        "name": "Draft",
        "node_type": "SCRIPT",
        "created_at": 1000,
        "updated_at": 1000
    });
    let (status, body) = call(&app, "POST", "/api/v1/templates/node/drafts", Some(draft)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"], "nt-draft");

    let (status, body) = call(&app, "GET", "/api/v1/templates/node/drafts/nt-draft", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["name"], "Draft");

    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/templates/node/drafts/nt-draft/promote",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = call(&app, "GET", "/api/v1/templates/node/nt-draft", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["id"], "nt-draft");

    let (status, _) = call(
        &app,
        "DELETE",
        "/api/v1/templates/node/drafts/nt-draft",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

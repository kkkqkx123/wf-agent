//! System surface: `/`, `/health`, `/system/*`, `/api/v1/info` and
//! `/api/v1/storage/*`.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

#[derive(serde::Serialize)]
pub struct HealthView {
    ready: bool,
    persistence: wf_api::PersistenceHealth,
    storage: String,
}

#[derive(serde::Serialize)]
struct InfoView {
    name: &'static str,
    version: &'static str,
    #[serde(rename = "apiVersion")]
    api_version: &'static str,
    timestamp: String,
}

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/", get(handle_root))
        .route("/health", get(handle_health))
        .route("/api/v1/info", get(handle_info))
        .route("/api/v1/storage/diagnose", get(handle_storage_diagnose))
        .route("/api/v1/storage/health", get(handle_storage_health))
        .route("/api/v1/storage/stats", get(handle_storage_stats))
        .route("/system/diagnostics", get(handle_diagnostics))
        .route("/system/event-health", get(handle_event_health))
}

async fn handle_root() -> impl IntoResponse {
    ok(serde_json::json!({
        "message": "Modular Agent Framework Server",
        "endpoints": {
            "health": "/health",
            "info": "/api/v1/info",
            "workflows": "/api/v1/workflows",
            "executions": "/api/v1/executions",
            "events": "/api/v1/events",
            "sse/stream (SSE)": "/api/v1/events/stream?executionId=<id>",
            "websocket (WS)": "/api/v1/ws"
        }
    }))
    .into_response()
}

async fn handle_info() -> impl IntoResponse {
    ok(InfoView {
        name: "Modular Agent Framework Server",
        version: env!("CARGO_PKG_VERSION"),
        api_version: "v1",
        timestamp: wf_common::time::timestamp_to_iso(wf_common::time::now()),
    })
    .into_response()
}

async fn handle_storage_diagnose(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::diagnostics::diagnose(&state.ctx).await {
        Ok(report) => ok(report).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_storage_health(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::diagnostics::health(&state.ctx).await {
        Ok(report) => ok(report).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_storage_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::diagnostics::item_counts(&state.ctx).await {
        Ok(counts) => ok(counts).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_health(State(state): State<ApiState>) -> impl IntoResponse {
    let snapshot = state.ctx.storage.ops_snapshot();
    let total_ops = snapshot.save.count()
        + snapshot.load.count()
        + snapshot.delete.count()
        + snapshot.list.count()
        + snapshot.exists.count()
        + snapshot.clear.count()
        + snapshot.batch.count();
    ok(HealthView {
        ready: true,
        persistence: state.ctx.persistence.health(),
        storage: format!("{total_ops} ops"),
    })
    .into_response()
}

async fn handle_diagnostics(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::diagnostics::health(&state.ctx).await {
        Ok(report) => ok(report).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_event_health(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::infra::events::event_system_health(&state.ctx).await {
        Ok(health) => ok(health).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::Request;
    use axum::response::Response;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        ))
    }

    async fn get(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn health_endpoint_reports_ready() {
        let ctx = make_ctx();
        let response = get(ctx, "/health").await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["ready"], true);
    }

    #[tokio::test]
    async fn system_endpoints_report_healthy() {
        let ctx = make_ctx();
        for uri in ["/system/diagnostics", "/system/event-health"] {
            let response = get(ctx.clone(), uri).await;
            assert_eq!(response.status(), axum::http::StatusCode::OK, "uri: {uri}");
            let body = json_body(response).await;
            assert_eq!(body["success"], true, "uri: {uri}");
        }

        let diagnostics = get(ctx, "/system/diagnostics").await;
        let body = json_body(diagnostics).await;
        assert_eq!(body["data"]["healthy"], true);
    }

    #[tokio::test]
    async fn root_and_info_endpoints_exist() {
        let ctx = make_ctx();
        let root = get(ctx.clone(), "/").await;
        assert_eq!(root.status(), axum::http::StatusCode::OK);
        let body = json_body(root).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["message"], "Modular Agent Framework Server");
        assert_eq!(body["data"]["endpoints"]["websocket (WS)"], "/api/v1/ws");

        let info = get(ctx, "/api/v1/info").await;
        assert_eq!(info.status(), axum::http::StatusCode::OK);
        let body = json_body(info).await;
        assert_eq!(body["data"]["apiVersion"], "v1");
        assert!(!body["data"]["timestamp"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn storage_endpoints_report_healthy() {
        let ctx = make_ctx();
        for uri in ["/api/v1/storage/health", "/api/v1/storage/diagnose"] {
            let response = get(ctx.clone(), uri).await;
            assert_eq!(response.status(), axum::http::StatusCode::OK, "uri: {uri}");
            let body = json_body(response).await;
            assert_eq!(body["success"], true, "uri: {uri}");
            assert_eq!(body["data"]["healthy"], true, "uri: {uri}");
        }

        let stats = get(ctx, "/api/v1/storage/stats").await;
        assert_eq!(stats.status(), axum::http::StatusCode::OK);
        let body = json_body(stats).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["workflow"], 0);
    }
}

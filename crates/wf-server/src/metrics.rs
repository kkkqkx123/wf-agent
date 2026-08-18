//! HTTP transport for metrics: Prometheus scrape endpoint plus server
//! lifecycle; the `/api/v1/metrics/*` REST surface lives in
//! `api/resource/metrics.rs` and is composed here through a nest.
//!
//! Kept independent of `wf-api` (pure service layer) and `wf-runtime`
//! (bootstrap); `serve` only needs a registry.
//!
//! Note: `/metrics` and `/export` render the in-memory snapshot of the
//! most recent `retention_ms` window (default 1h), not process-cumulative
//! values. Scrapers must interpret the output as a window snapshot.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use wf_metrics::{format_registry_prometheus, MetricsRegistry};

use crate::api::resource::metrics::{self, RegistryState, PROMETHEUS_CONTENT_TYPE};
use crate::server::{serve_with_router, ServeError, ServerHandle};

/// Build the metrics router. Exposed for embedding and testing.
pub fn router(registry: Arc<MetricsRegistry>) -> Router {
    let state = RegistryState { registry };
    let app: Router<RegistryState> = Router::new()
        .route("/metrics", get(handle_metrics))
        .nest("/api/v1/metrics", metrics::routes());
    app.with_state(state)
}

/// Serve the metrics API on `addr` without blocking.
pub async fn serve(
    registry: Arc<MetricsRegistry>,
    addr: SocketAddr,
) -> Result<ServerHandle, ServeError> {
    serve_with_router(router(registry), addr).await
}

/// `GET /metrics`: Prometheus text export, `text/plain`.
async fn handle_metrics(State(state): State<RegistryState>) -> Response {
    text_response(
        PROMETHEUS_CONTENT_TYPE,
        format_registry_prometheus(&state.registry),
    )
}

fn text_response(content_type: &str, body: String) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type).expect("valid content type"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn seeded_registry() -> Arc<MetricsRegistry> {
        let registry = Arc::new(MetricsRegistry::new());
        registry.workflow().record_execution_start("wf-1");
        registry
            .workflow()
            .record_execution_complete("wf-1", None, true, 100.0, None);
        registry
            .node()
            .record_execution(wf_metrics::collectors::node::NodeExecutionRecord {
                node_id: "n1",
                node_type: "Llm",
                execution_id: "e1",
                success: true,
                duration_ms: 30.0,
                input_size: 10,
                output_size: 20,
                error_type: None,
            });
        registry.agent().record_execution_start("default");
        registry
            .agent()
            .record_execution_complete("default", true, 100.0);
        registry.agent_loop().record_iteration(50.0);
        registry.error().record_error("llm", "agent", Some("e1"));
        registry.tool().record_tool_call_start("http", "e1");
        registry
            .token()
            .record_token_usage(10, 5, Some(0.002), Some("gpt-4o"));
        registry.config().record_access();
        registry.resource().record_memory_usage(1024);
        registry
            .event()
            .record_event("NodeStarted", Some("e1"), Some("wf-1"));
        registry
            .event()
            .record_event("NodeCompleted", Some("e1"), Some("wf-1"));
        registry
    }

    async fn get(registry: Arc<MetricsRegistry>, uri: &str) -> Response {
        router(registry)
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn metrics_endpoint_returns_prometheus_text() {
        let response = get(seeded_registry(), "/metrics").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            PROMETHEUS_CONTENT_TYPE
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("workflow.execution.count"));
        assert!(text.contains("error.occurrence.count"));
    }

    #[tokio::test]
    async fn workflow_endpoint_scopes_by_workflow_id() {
        let registry = seeded_registry();
        registry.workflow().record_execution_start("wf-2");
        registry
            .workflow()
            .record_execution_complete("wf-2", None, false, 10.0, None);

        let response = get(
            registry.clone(),
            "/api/v1/metrics/workflow?workflow_id=wf-2",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["total"], 1);
        assert_eq!(json["data"]["failure"], 1);
    }

    #[tokio::test]
    async fn node_templates_endpoint_applies_top_n() {
        let registry = seeded_registry();
        registry
            .node()
            .record_execution(wf_metrics::collectors::node::NodeExecutionRecord {
                node_id: "n2",
                node_type: "Script",
                execution_id: "e1",
                success: true,
                duration_ms: 5.0,
                input_size: 1,
                output_size: 1,
                error_type: None,
            });
        registry
            .node()
            .record_execution(wf_metrics::collectors::node::NodeExecutionRecord {
                node_id: "n3",
                node_type: "Llm",
                execution_id: "e2",
                success: true,
                duration_ms: 5.0,
                input_size: 1,
                output_size: 1,
                error_type: None,
            });
        let response = get(registry, "/api/v1/metrics/node-templates?top_n=1").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let types = json["data"]["by_node_type"].as_array().unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0]["labels"]["node_type"], "Llm");
    }

    #[tokio::test]
    async fn agents_endpoint_scopes_by_profile() {
        let registry = seeded_registry();
        registry.agent().record_execution_start("alt");
        registry
            .agent()
            .record_execution_complete("alt", false, 5.0);

        let response = get(registry, "/api/v1/metrics/agents?profile_id=alt").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["total"], 1);
        assert_eq!(json["data"]["failure"], 1);
    }

    #[tokio::test]
    async fn report_endpoint_returns_full_report() {
        let response = get(seeded_registry(), "/api/v1/metrics/report").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert!(json["data"]["summary"]["total_metrics"].as_u64().unwrap() > 0);
        assert!(!json["data"]["top_metrics"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn export_formats_match_content_types() {
        let registry = seeded_registry();
        let response = get(registry.clone(), "/api/v1/metrics/export?format=prometheus").await;
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            PROMETHEUS_CONTENT_TYPE
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("# HELP workflow.execution.count"));

        let response = get(registry, "/api/v1/metrics/export?format=json").await;
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.is_array());
        assert!(json.as_array().unwrap().len() >= 8);
    }

    #[tokio::test]
    async fn export_rejects_unknown_format() {
        let response = get(seeded_registry(), "/api/v1/metrics/export?format=parquet").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["error"]["code"], "INVALID_PARAMS");
    }

    #[tokio::test]
    async fn collectors_endpoint_lists_detail() {
        let response = get(seeded_registry(), "/api/v1/metrics/collectors").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert!(json["data"].as_array().unwrap().len() >= 8);
    }
}

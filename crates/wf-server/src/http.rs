//! HTTP transport for metrics: Prometheus scrape endpoint plus REST stats
//! endpoints over the `wf-metrics` registry.
//!
//! Kept independent of `wf-api` (pure service layer) and `wf-runtime`
//! (bootstrap); `serve` only needs a registry.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use wf_metrics::collectors::{AgentUsageStats, NodeUsageStats, WorkflowUsageStats};
use wf_metrics::{
    format_registry_json, format_registry_prometheus, generate_report, MetricReport,
    MetricsRegistry, ReportOptions,
};

const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("bind failed: {0}")]
    Bind(String),
    #[error("server error: {0}")]
    Server(String),
}

/// Error classification used by the response envelope. The full mapping
/// (NotFound -> 404, Validation -> 400, anything else -> 500) is the API
/// contract; today only Validation is reachable from the handlers.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    NotFound,
    Validation,
    Internal,
}

impl ErrorKind {
    fn status(self) -> StatusCode {
        match self {
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Validation => StatusCode::BAD_REQUEST,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(self) -> &'static str {
        match self {
            ErrorKind::NotFound => "NOT_FOUND",
            ErrorKind::Validation => "INVALID_PARAMS",
            ErrorKind::Internal => "INTERNAL_ERROR",
        }
    }
}

struct ApiError {
    kind: ErrorKind,
    message: String,
}

impl ApiError {
    fn validation(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Validation,
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
struct ApiErrorBody {
    code: String,
    message: String,
}

#[derive(Serialize)]
struct ApiEnvelope<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
}

fn ok<T: Serialize>(data: T) -> Json<ApiEnvelope<T>> {
    Json(ApiEnvelope {
        success: true,
        data: Some(data),
        error: None,
    })
}

fn err<T: Serialize>(e: ApiError) -> (StatusCode, Json<ApiEnvelope<T>>) {
    (
        e.kind.status(),
        Json(ApiEnvelope {
            success: false,
            data: None,
            error: Some(ApiErrorBody {
                code: e.kind.code().to_string(),
                message: e.message,
            }),
        }),
    )
}

#[derive(Clone)]
struct AppState {
    registry: Arc<MetricsRegistry>,
}

/// Build the metrics router. Exposed for embedding and testing.
pub fn router(registry: Arc<MetricsRegistry>) -> Router {
    Router::new()
        .route("/metrics", get(handle_metrics))
        .route("/api/v1/metrics/workflow", get(handle_workflow))
        .route("/api/v1/metrics/node-templates", get(handle_node_templates))
        .route("/api/v1/metrics/agents", get(handle_agents))
        .route("/api/v1/metrics/report", get(handle_report))
        .route("/api/v1/metrics/export", get(handle_export))
        .route("/api/v1/metrics/collectors", get(handle_collectors))
        .with_state(AppState { registry })
}

/// Serve the metrics API on `addr` until the process stops.
pub async fn serve(registry: Arc<MetricsRegistry>, addr: SocketAddr) -> Result<(), ServeError> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| ServeError::Bind(e.to_string()))?;
    axum::serve(listener, router(registry))
        .await
        .map_err(|e| ServeError::Server(e.to_string()))
}

/// `GET /metrics`: Prometheus text export, `text/plain` (TS exported JSON
/// with the Prometheus content-type; corrected here).
async fn handle_metrics(State(state): State<AppState>) -> Response {
    text_response(
        PROMETHEUS_CONTENT_TYPE,
        format_registry_prometheus(&state.registry),
    )
}

#[derive(Deserialize)]
struct WorkflowQuery {
    workflow_id: Option<String>,
}

async fn handle_workflow(
    State(state): State<AppState>,
    Query(query): Query<WorkflowQuery>,
) -> impl IntoResponse {
    let stats: WorkflowUsageStats = match query.workflow_id {
        Some(id) => state.registry.workflow().usage_stats_for(&id),
        None => state.registry.workflow().usage_stats(),
    };
    ok(stats).into_response()
}

#[derive(Deserialize)]
struct NodeTemplatesQuery {
    top_n: Option<usize>,
}

async fn handle_node_templates(
    State(state): State<AppState>,
    Query(query): Query<NodeTemplatesQuery>,
) -> impl IntoResponse {
    let top_n = query.top_n.unwrap_or(10);
    let stats = state.registry.node().usage_stats();
    let mut by_node_type = stats.by_node_type;
    by_node_type.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    by_node_type.truncate(top_n);
    ok(NodeUsageStats {
        by_node_type,
        ..stats
    })
    .into_response()
}

#[derive(Deserialize)]
struct AgentsQuery {
    profile_id: Option<String>,
}

async fn handle_agents(
    State(state): State<AppState>,
    Query(query): Query<AgentsQuery>,
) -> impl IntoResponse {
    let stats: AgentUsageStats = match query.profile_id {
        Some(id) => state.registry.agent().usage_stats_for(&id),
        None => state.registry.agent().usage_stats(),
    };
    ok(stats).into_response()
}

async fn handle_report(State(state): State<AppState>) -> impl IntoResponse {
    let report: MetricReport = generate_report(&state.registry, &ReportOptions::default()).await;
    ok(report).into_response()
}

#[derive(Deserialize)]
struct ExportQuery {
    format: Option<String>,
}

async fn handle_export(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> Response {
    match query.format.as_deref() {
        Some("json") | None => Json(format_registry_json(&state.registry)).into_response(),
        Some("prometheus") => text_response(
            PROMETHEUS_CONTENT_TYPE,
            format_registry_prometheus(&state.registry),
        ),
        Some(other) => err::<serde_json::Value>(ApiError::validation(format!(
            "unsupported export format: {other}"
        )))
        .into_response(),
    }
}

async fn handle_collectors(State(state): State<AppState>) -> impl IntoResponse {
    ok(format_registry_json(&state.registry)).into_response()
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
    use axum::http::Request;
    use tower::ServiceExt;

    fn seeded_registry() -> Arc<MetricsRegistry> {
        let registry = Arc::new(MetricsRegistry::new());
        registry.workflow().record_execution_start("e1", "wf-1");
        registry
            .workflow()
            .record_execution_complete("e1", "wf-1", None, true, 100.0, None);
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
        registry.agent().record_execution_start("default", "e1");
        registry
            .agent()
            .record_execution_complete("default", "e1", true, 100.0);
        registry.agent_loop().record_iteration("e1", 50.0);
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
        registry.workflow().record_execution_start("e2", "wf-2");
        registry
            .workflow()
            .record_execution_complete("e2", "wf-2", None, false, 10.0, None);

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
        registry.agent().record_execution_start("alt", "e2");
        registry
            .agent()
            .record_execution_complete("alt", "e2", false, 5.0);

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

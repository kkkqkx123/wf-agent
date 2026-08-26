//! REST metrics endpoints under `/api/v1/metrics/*` (workflow,
//! node-templates, agents, report, export, collectors). Serves as the
//! transport layer over the `wf-metrics` registry; `metrics.rs` keeps the
//! Prometheus `/metrics` endpoint and server lifecycle, composing this
//! module through a nest.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use wf_metrics::collectors::{AgentUsageStats, NodeUsageStats, WorkflowUsageStats};
use wf_metrics::{
    format_registry_json, format_registry_prometheus, generate_report, MetricReport,
    MetricsRegistry, ReportOptions,
};

use crate::envelope::{err, ApiError};

pub(crate) const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

#[derive(Clone)]
pub(crate) struct RegistryState {
    pub(crate) registry: Arc<MetricsRegistry>,
}

/// REST metrics routes, mounted by `metrics.rs` under `/api/v1/metrics`;
/// the caller supplies the shared registry through `with_state`.
pub(crate) fn routes() -> Router<RegistryState> {
    Router::new()
        .route("/workflow", get(handle_workflow))
        .route("/node-templates", get(handle_node_templates))
        .route("/agents", get(handle_agents))
        .route("/report", get(handle_report))
        .route("/export", get(handle_export))
        .route("/collectors", get(handle_collectors))
}

#[derive(Deserialize)]
struct WorkflowQuery {
    workflow_id: Option<String>,
}

async fn handle_workflow(
    State(state): State<RegistryState>,
    Query(query): Query<WorkflowQuery>,
) -> impl IntoResponse {
    let stats: WorkflowUsageStats = match query.workflow_id {
        Some(id) => state.registry.workflow().usage_stats_for(&id),
        None => state.registry.workflow().usage_stats(),
    };
    crate::envelope::ok(stats).into_response()
}

#[derive(Deserialize)]
struct NodeTemplatesQuery {
    top_n: Option<usize>,
}

async fn handle_node_templates(
    State(state): State<RegistryState>,
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
    crate::envelope::ok(NodeUsageStats {
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
    State(state): State<RegistryState>,
    Query(query): Query<AgentsQuery>,
) -> impl IntoResponse {
    let stats: AgentUsageStats = match query.profile_id {
        Some(id) => state.registry.agent().usage_stats_for(&id),
        None => state.registry.agent().usage_stats(),
    };
    crate::envelope::ok(stats).into_response()
}

async fn handle_report(State(state): State<RegistryState>) -> impl IntoResponse {
    let report: MetricReport = generate_report(&state.registry, &ReportOptions::default()).await;
    crate::envelope::ok(report).into_response()
}

#[derive(Deserialize)]
struct ExportQuery {
    format: Option<String>,
}

async fn handle_export(
    State(state): State<RegistryState>,
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

async fn handle_collectors(State(state): State<RegistryState>) -> impl IntoResponse {
    crate::envelope::ok(format_registry_json(&state.registry)).into_response()
}

fn text_response(content_type: &str, body: String) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type).expect("valid content type"),
    );
    response
}

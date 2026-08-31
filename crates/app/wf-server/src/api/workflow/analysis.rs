//! Analysis domain: execution progress, unified cross-resource search, LLM
//! metrics, workflow error analysis, performance profiling and aggregate
//! stats. Handlers stay thin; every payload comes from `wf-api::analysis`.

use std::convert::Infallible;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;

use wf_api::analysis::search::SearchOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{IdErrorPath, IdPath};
use crate::router::ApiState;
use crate::sse::sse_response;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/executions/{id}/progress", get(handle_progress))
        .route("/search", get(handle_search))
        .route("/analysis/llm-metrics", get(handle_llm_metrics))
        .route(
            "/analysis/performance/compare",
            get(handle_performance_compare),
        )
        .route("/analysis/stats", get(handle_stats))
        .route("/analysis/stats/top-workflows", get(handle_top_workflows))
        .route("/analysis/stats/top-node-types", get(handle_top_node_types))
        .route(
            "/analysis/stats/agent-profiles",
            get(handle_agent_stats_by_profile),
        )
        .route(
            "/executions/{id}/error-analysis",
            get(handle_error_analysis),
        )
        .route(
            "/executions/{id}/error-analysis/advanced",
            get(handle_error_analysis_advanced),
        )
        .route(
            "/executions/{id}/error-analysis/root-cause",
            get(handle_error_root_cause),
        )
        .route(
            "/executions/{id}/error-analysis/context",
            get(handle_error_context),
        )
        .route(
            "/executions/{id}/error-analysis/context/{errorId}",
            get(handle_error_context_one),
        )
        .route(
            "/executions/{id}/error-analysis/recovery-recommendations",
            get(handle_error_recovery_recommendations),
        )
        .route(
            "/executions/{id}/error-analysis/recovery/{errorId}",
            get(handle_error_recovery),
        )
        .route(
            "/executions/{id}/error-analysis/similar",
            get(handle_error_similar),
        )
        .route(
            "/executions/{id}/error-analysis/stream",
            get(handle_error_chain_stream),
        )
        .route("/executions/{id}/performance", get(handle_performance))
        .route(
            "/executions/{id}/performance/summary",
            get(handle_performance_summary),
        )
        .route(
            "/executions/{id}/performance/bottlenecks",
            get(handle_performance_bottlenecks),
        )
        .route(
            "/executions/{id}/performance/iteration-comparison",
            get(handle_iteration_comparison),
        )
}

async fn handle_progress(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::progress::get_progress(&state.ctx, &path.id).await {
        Ok(progress) => ok(progress).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    types: Option<String>,
    limit_per_type: Option<usize>,
    limit: Option<usize>,
}

async fn handle_search(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let types = query
        .types
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .filter_map(|t| match t.trim() {
                    "workflow" => Some(wf_api::SearchResourceType::Workflow),
                    "execution" => Some(wf_api::SearchResourceType::Execution),
                    "task" => Some(wf_api::SearchResourceType::Task),
                    "checkpoint" => Some(wf_api::SearchResourceType::Checkpoint),
                    "event" => Some(wf_api::SearchResourceType::Event),
                    "agent_loop" => Some(wf_api::SearchResourceType::AgentLoop),
                    _ => None,
                })
                .collect()
        })
        .filter(|types: &Vec<_>| !types.is_empty());
    let options = SearchOptions {
        types,
        limit_per_type: query.limit_per_type,
        limit_total: query.limit,
    };
    match wf_api::analysis::search::search(&state.ctx, &query.q, &options).await {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_llm_metrics(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::analysis::llm_metrics::agent_llm_metrics(&state.ctx).await {
        Ok(metrics) => ok(metrics).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct CompareQuery {
    baseline: String,
    compared: String,
}

async fn handle_performance_compare(
    State(state): State<ApiState>,
    Query(query): Query<CompareQuery>,
) -> impl IntoResponse {
    match wf_api::analysis::performance::compare(&state.ctx, &query.baseline, &query.compared).await
    {
        Ok(comparison) => ok(comparison).into_response(),
        Err(e) => error_response(e),
    }
}

/// Aggregate usage stats across workflow / node / agent / tool / error /
/// event collectors. `configured: false` when the context carries no
/// metrics registry (the default in-memory context).
async fn handle_stats(State(state): State<ApiState>) -> impl IntoResponse {
    let Some(metrics) = state.ctx.metrics.as_ref() else {
        return ok(json!({ "configured": false })).into_response();
    };
    ok(json!({
        "configured": true,
        "workflow": wf_api::analysis::stats::workflow_stats(metrics),
        "node": wf_api::analysis::stats::node_stats(metrics),
        "agent": wf_api::analysis::stats::agent_stats(metrics),
        "tool": wf_api::analysis::stats::tool_stats(metrics),
        "error": wf_api::analysis::stats::error_stats(metrics),
        "event": wf_api::analysis::stats::event_stats(metrics),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct TopStatsQuery {
    limit: Option<usize>,
}

async fn handle_top_workflows(
    State(state): State<ApiState>,
    Query(query): Query<TopStatsQuery>,
) -> impl IntoResponse {
    let Some(metrics) = state.ctx.metrics.as_ref() else {
        return ok(json!({ "configured": false })).into_response();
    };
    let limit = query.limit.unwrap_or(10);
    match wf_api::workflow::list_workflows(&state.ctx, None).await {
        Ok(workflows) => {
            let ids: Vec<String> = workflows.into_iter().map(|w| w.id).collect();
            ok(wf_api::analysis::stats::top_workflows(metrics, &ids, limit)).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_top_node_types(
    State(state): State<ApiState>,
    Query(query): Query<TopStatsQuery>,
) -> impl IntoResponse {
    let Some(metrics) = state.ctx.metrics.as_ref() else {
        return ok(json!({ "configured": false })).into_response();
    };
    let limit = query.limit.unwrap_or(10);
    ok(wf_api::analysis::stats::top_node_types(metrics, limit)).into_response()
}

async fn handle_agent_stats_by_profile(
    State(state): State<ApiState>,
    Query(query): Query<TopStatsQuery>,
) -> impl IntoResponse {
    let Some(metrics) = state.ctx.metrics.as_ref() else {
        return ok(json!({ "configured": false })).into_response();
    };
    let limit = query.limit.unwrap_or(10);
    match wf_api::agent::agent::list_agent_profiles(&state.ctx.storage, None).await {
        Ok(profiles) => {
            let ids: Vec<String> = profiles.into_iter().map(|p| p.id.to_string()).collect();
            ok(wf_api::analysis::stats::agent_stats_by_profile(
                metrics, &ids, limit,
            ))
            .into_response()
        }
        Err(e) => error_response(e),
    }
}

// ── error analysis ────────────────────────────────────────────────

async fn handle_error_analysis(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::error_analysis::workflow_error_stats(&state.ctx, &path.id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_error_analysis_advanced(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::error_analysis::get_advanced_error_analysis(&state.ctx, &path.id).await
    {
        Ok(analysis) => ok(analysis).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_error_root_cause(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::error_analysis::analyze_root_cause(&state.ctx, &path.id).await {
        Ok(analysis) => ok(analysis).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_error_context(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::error_analysis::error_context_chain(&state.ctx, &path.id).await {
        Ok(contexts) => ok(contexts).into_response(),
        Err(e) => error_response(e),
    }
}

/// Context of a single error of an execution (error record plus the
/// recorded execution state around its timestamp).
async fn handle_error_context_one(
    State(state): State<ApiState>,
    Path(path): Path<IdErrorPath>,
) -> impl IntoResponse {
    match wf_api::analysis::error_analysis::error_context(&state.ctx, &path.id, &path.error_id)
        .await
    {
        Ok(context) => ok(context).into_response(),
        Err(e) => error_response(e),
    }
}

/// Recovery recommendations of an execution (one per error record carrying
/// a recovery action).
async fn handle_error_recovery_recommendations(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::error_analysis::recovery_recommendations(&state.ctx, &path.id).await {
        Ok(recommendations) => ok(recommendations).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_error_recovery(
    State(state): State<ApiState>,
    Path(path): Path<IdErrorPath>,
) -> impl IntoResponse {
    match wf_api::analysis::error_analysis::get_recovery_proposal(
        &state.ctx,
        &path.id,
        &path.error_id,
    )
    .await
    {
        Ok(proposal) => ok(proposal).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SimilarErrorsQuery {
    limit: Option<usize>,
}

async fn handle_error_similar(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<SimilarErrorsQuery>,
) -> impl IntoResponse {
    match wf_api::analysis::error_analysis::similar_errors(
        &state.ctx,
        &path.id,
        query.limit.unwrap_or(10),
    )
    .await
    {
        Ok(errors) => ok(errors).into_response(),
        Err(e) => error_response(e),
    }
}

/// Stream the root-first error chain of an execution as SSE frames.
/// Degrades to an empty stream (connection closes) when the execution
/// has no recorded errors.
async fn handle_error_chain_stream(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> Response {
    let stream =
        match wf_api::analysis::error_analysis::stream_error_chain(&state.ctx, &path.id).await {
            Ok(stream) => stream,
            Err(e) => return error_response(e),
        };
    let events = futures::stream::unfold(stream, |mut stream| async move {
        match stream.next().await {
            Some(record) => {
                let payload = serde_json::to_string(&record).unwrap_or_else(|_| "{}".to_string());
                let frame = format!("data: {payload}\n\n");
                Some((Ok::<_, Infallible>(Bytes::from(frame)), stream))
            }
            None => None,
        }
    });
    sse_response(events)
}

// ── performance ───────────────────────────────────────────────────

async fn handle_performance(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::performance::profile(&state.ctx, &path.id).await {
        Ok(profile) => ok(profile).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_performance_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::performance::get_performance_summary(&state.ctx, &path.id).await {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_performance_bottlenecks(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::performance::identify_bottlenecks(&state.ctx, &path.id).await {
        Ok(bottlenecks) => ok(bottlenecks).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_iteration_comparison(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::analysis::performance::get_iteration_comparison(&state.ctx, &path.id).await {
        Ok(comparison) => ok(comparison).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
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

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn analysis_endpoints_are_reachable() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/executions/exec-1/progress",
            "/api/v1/search?q=hello",
            "/api/v1/search?q=hello&types=workflow,execution",
            "/api/v1/analysis/llm-metrics",
            "/api/v1/analysis/stats",
            "/api/v1/analysis/stats/top-workflows",
            "/api/v1/analysis/stats/top-node-types",
            "/api/v1/analysis/stats/agent-profiles",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
        // Error / performance analysis degrades to empty results for
        // unknown executions.
        for uri in [
            "/api/v1/executions/exec-1/error-analysis",
            "/api/v1/executions/exec-1/error-analysis/advanced",
            "/api/v1/executions/exec-1/error-analysis/root-cause",
            "/api/v1/executions/exec-1/error-analysis/context",
            "/api/v1/executions/exec-1/error-analysis/recovery-recommendations",
            "/api/v1/executions/exec-1/error-analysis/similar",
            "/api/v1/executions/exec-1/error-analysis/stream",
            "/api/v1/executions/exec-1/performance",
            "/api/v1/executions/exec-1/performance/summary",
            "/api/v1/executions/exec-1/performance/bottlenecks",
            "/api/v1/executions/exec-1/performance/iteration-comparison",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
        // A single error context for an unknown error id is a NotFound.
        let missing = send(
            ctx.clone(),
            "/api/v1/executions/exec-1/error-analysis/context/err-1",
        )
        .await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn performance_compare_degrades_to_empty() {
        let ctx = make_ctx();
        let response = send(
            ctx,
            "/api/v1/analysis/performance/compare?baseline=missing-a&compared=missing-b",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}

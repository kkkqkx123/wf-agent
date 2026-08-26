//! Agent analysis subface: execution error analysis and performance
//! analysis. The decision graph subface lives in `api_agent_graph`.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::extract::{IdErrorPath, IdPath};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .merge(crate::api::agent::graphs::routes())
        // ── error analysis ──
        .route("/agent-executions/{id}/errors", get(handle_error_records))
        .route(
            "/agent-executions/{id}/errors/chain",
            get(handle_error_chain),
        )
        .route(
            "/agent-executions/{id}/errors/root-cause",
            get(handle_root_cause),
        )
        .route(
            "/agent-executions/{id}/errors/statistics",
            get(handle_error_statistics),
        )
        .route(
            "/agent-executions/{id}/errors/statistics/advanced",
            get(handle_advanced_error_statistics),
        )
        .route(
            "/agent-executions/{id}/errors/recovery/{errorId}",
            get(handle_recovery_proposal),
        )
        .route(
            "/agent-executions/{id}/errors/similar/{errorId}",
            get(handle_similar_errors),
        )
        // ── performance ──
        .route("/agent-loops/{id}/performance", get(handle_performance))
        .route(
            "/agent-loops/{id}/performance/comparison",
            get(handle_iteration_comparison),
        )
}

// ── error analysis ────────────────────────────────────────────────

async fn handle_error_records(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_error_analysis::get_execution_error_records(&state.ctx, &path.id)
        .await
    {
        Ok(records) => ok(records).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ErrorChainQuery {
    from_error_id: Option<String>,
}

async fn handle_error_chain(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ErrorChainQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_error_analysis::get_error_chain(
        &state.ctx,
        &path.id,
        query.from_error_id.as_deref(),
    )
    .await
    {
        Ok(chain) => ok(chain).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_root_cause(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_error_analysis::analyze_root_cause(&state.ctx, &path.id).await {
        Ok(analysis) => ok(analysis).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_error_statistics(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_error_analysis::get_error_statistics(&state.ctx, &path.id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_advanced_error_statistics(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_error_analysis::get_advanced_error_analysis(&state.ctx, &path.id)
        .await
    {
        Ok(analysis) => ok(analysis).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_recovery_proposal(
    State(state): State<ApiState>,
    Path(path): Path<IdErrorPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_error_analysis::get_recovery_proposal(
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

async fn handle_similar_errors(
    State(state): State<ApiState>,
    Path(path): Path<IdErrorPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_error_analysis::get_similar_errors(
        &state.ctx,
        &path.id,
        &path.error_id,
    )
    .await
    {
        Ok(errors) => ok(errors).into_response(),
        Err(e) => error_response(e),
    }
}

// ── performance ───────────────────────────────────────────────────

async fn handle_performance(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_performance::analyze_performance(&state.ctx, &path.id).await {
        Ok(profile) => ok(profile).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_iteration_comparison(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_performance::iteration_comparison(&state.ctx, &path.id).await {
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
    async fn agent_analysis_endpoints_are_reachable() {
        let ctx = make_ctx();
        // Analysis of unknown executions degrades to empty results (data
        // lives on the execution record / live entity).
        for uri in [
            "/api/v1/agent-executions/exec-1/errors",
            "/api/v1/agent-executions/exec-1/errors/chain",
            "/api/v1/agent-executions/exec-1/errors/root-cause",
            "/api/v1/agent-executions/exec-1/errors/statistics",
            "/api/v1/agent-executions/exec-1/errors/statistics/advanced",
            "/api/v1/agent-loops/loop-1/graph",
            "/api/v1/agent-loops/loop-1/graph/nodes",
            "/api/v1/agent-loops/loop-1/graph/edges",
            "/api/v1/agent-loops/loop-1/graph/paths",
            "/api/v1/agent-loops/loop-1/graph/alternatives",
            "/api/v1/agent-loops/loop-1/graph/alternatives/iterations/1",
            "/api/v1/agent-loops/loop-1/graph/sequences",
            "/api/v1/agent-loops/loop-1/graph/sequences/iterations/1",
            "/api/v1/agent-loops/loop-1/graph/sequences/types/tool_selection",
            "/api/v1/agent-loops/loop-1/graph/paths/steps",
            "/api/v1/agent-loops/loop-1/graph/tool-frequency",
            "/api/v1/agent-loops/loop-1/graph/unexplored",
            "/api/v1/agent-loops/loop-1/graph/unexplored/best",
            "/api/v1/agent-loops/loop-1/graph/patterns",
            "/api/v1/agent-loops/loop-1/graph/efficiency",
            "/api/v1/agent-loops/loop-1/graph/probabilities",
            "/api/v1/agent-loops/loop-1/performance",
            "/api/v1/agent-loops/loop-1/performance/comparison",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
    }
}

//! Agent decision graph subface: graph / nodes / edges / paths /
//! alternatives / sequences / unexplored / tool-frequency / patterns /
//! efficiency / probabilities. Split from `api_agent_analysis` to keep the
//! agent analysis surface at a maintainable file size.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/agent-loops/{id}/graph", get(handle_decision_graph))
        .route("/agent-loops/{id}/graph/nodes", get(handle_decision_nodes))
        .route("/agent-loops/{id}/graph/edges", get(handle_decision_edges))
        .route("/agent-loops/{id}/graph/paths", get(handle_all_paths))
        .route(
            "/agent-loops/{id}/graph/paths/execution-path",
            get(handle_graph_execution_path),
        )
        .route(
            "/agent-loops/{id}/graph/paths/path-stats",
            get(handle_path_statistics),
        )
        .route(
            "/agent-loops/{id}/graph/paths/critical-path",
            get(handle_critical_path),
        )
        .route(
            "/agent-loops/{id}/graph/alternatives",
            get(handle_all_alternatives),
        )
        .route(
            "/agent-loops/{id}/graph/alternatives/iterations/{iteration}",
            get(handle_alternative_decisions),
        )
        .route(
            "/agent-loops/{id}/graph/sequences",
            get(handle_decision_sequence),
        )
        .route(
            "/agent-loops/{id}/graph/sequences/iterations/{iteration}",
            get(handle_decisions_in_iteration),
        )
        .route(
            "/agent-loops/{id}/graph/sequences/types/{decisionType}",
            get(handle_decisions_by_type),
        )
        .route(
            "/agent-loops/{id}/graph/unexplored",
            get(handle_unexplored_alternatives),
        )
        .route(
            "/agent-loops/{id}/graph/unexplored/best",
            get(handle_most_promising_unexplored),
        )
        .route(
            "/agent-loops/{id}/graph/paths/steps",
            get(handle_execution_path_steps),
        )
        .route(
            "/agent-loops/{id}/graph/tool-frequency",
            get(handle_tool_frequency),
        )
        .route(
            "/agent-loops/{id}/graph/patterns",
            get(handle_decision_patterns),
        )
        .route(
            "/agent-loops/{id}/graph/efficiency",
            get(handle_path_efficiency),
        )
        .route(
            "/agent-loops/{id}/graph/probabilities",
            get(handle_path_probabilities),
        )
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Decision graph", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_decision_graph(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::decision_graph(&state.ctx, &path.id).await {
        Ok(graph) => ok(graph).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/nodes",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Decision nodes", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_decision_nodes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::decision_nodes(&state.ctx, &path.id).await {
        Ok(nodes) => ok(nodes).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/edges",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Decision edges", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_decision_edges(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::decision_edges(&state.ctx, &path.id).await {
        Ok(edges) => ok(edges).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/paths",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "All paths", body = crate::envelope::ApiEnvelope<crate::api::agent::graphs::CappedPaths>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_all_paths(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::all_paths(&state.ctx, &path.id).await {
        Ok(paths) => {
            let total = paths.len();
            // Aligned with execution_graph::MAX_ENUMERATED_PATHS; the domain
            // DFS already caps at 1000, the flag makes the cap explicit.
            let truncated = total >= wf_api::workflow::execution_graph::MAX_ENUMERATED_PATHS;
            ok(CappedPaths {
                paths,
                truncated,
                total,
            })
            .into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct CappedPaths {
    paths: Vec<Vec<String>>,
    truncated: bool,
    total: usize,
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/paths/execution-path",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Execution path", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_execution_path(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::execution_path(&state.ctx, &path.id).await {
        Ok(path_view) => ok(path_view).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/paths/path-stats",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Path statistics", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_path_statistics(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::path_statistics(&state.ctx, &path.id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/paths/critical-path",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Critical path", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_critical_path(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::critical_path(&state.ctx, &path.id).await {
        Ok(path_view) => ok(path_view).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/alternatives",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "All alternatives", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_all_alternatives(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::all_alternatives(&state.ctx, &path.id).await {
        Ok(alternatives) => ok(alternatives).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct IterationPath {
    id: String,
    iteration: u32,
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/alternatives/iterations/{iteration}",
    tag = "agent",
    params(
        ("id" = String, Path, description = "Agent loop ID"),
        ("iteration" = u32, Path, description = "Iteration number")
    ),
    responses(
        (status = 200, description = "Alternative decisions for iteration", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_alternative_decisions(
    State(state): State<ApiState>,
    Path(path): Path<IterationPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::alternative_decisions(&state.ctx, &path.id, path.iteration)
        .await
    {
        Ok(alternatives) => ok(alternatives).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/sequences",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Decision sequence", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_decision_sequence(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::decision_sequence(&state.ctx, &path.id).await {
        Ok(sequence) => ok(sequence).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/sequences/iterations/{iteration}",
    tag = "agent",
    params(
        ("id" = String, Path, description = "Agent loop ID"),
        ("iteration" = u32, Path, description = "Iteration number")
    ),
    responses(
        (status = 200, description = "Decisions in iteration", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_decisions_in_iteration(
    State(state): State<ApiState>,
    Path(path): Path<IterationPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::decisions_in_iteration(&state.ctx, &path.id, path.iteration)
        .await
    {
        Ok(decisions) => ok(decisions).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DecisionTypePath {
    id: String,
    decision_type: String,
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/sequences/types/{decisionType}",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID"), ("decisionType" = String, Path, description = "Decision type")),
    responses(
        (status = 200, description = "Decisions by type", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_decisions_by_type(
    State(state): State<ApiState>,
    Path(path): Path<DecisionTypePath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::decisions_by_type(&state.ctx, &path.id, &path.decision_type)
        .await
    {
        Ok(decisions) => ok(decisions).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/unexplored",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Unexplored alternatives", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unexplored_alternatives(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::unexplored_alternatives(&state.ctx, &path.id).await {
        Ok(alternatives) => ok(alternatives).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/unexplored/best",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Most promising unexplored alternative", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_most_promising_unexplored(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::most_promising_unexplored(&state.ctx, &path.id).await {
        Ok(alternative) => ok(alternative).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/paths/steps",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Execution path steps", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execution_path_steps(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::execution_path_steps(&state.ctx, &path.id).await {
        Ok(steps) => ok(steps).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/tool-frequency",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Tool frequency", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_tool_frequency(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::analyze(&state.ctx, &path.id).await {
        Ok(graph) => ok(wf_api::agent::agent_graph::tool_frequency(
            &state.ctx, &graph,
        ))
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/patterns",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Decision patterns", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_decision_patterns(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::analyze_decision_patterns(&state.ctx, &path.id).await {
        Ok(patterns) => ok(patterns).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/efficiency",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Path efficiency", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_path_efficiency(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::analyze_path_efficiency(&state.ctx, &path.id).await {
        Ok(efficiency) => ok(efficiency).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agent-loops/{id}/graph/probabilities",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Path probabilities", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_path_probabilities(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_graph::path_probability_analysis(&state.ctx, &path.id).await {
        Ok(probabilities) => ok(probabilities).into_response(),
        Err(e) => error_response(e),
    }
}

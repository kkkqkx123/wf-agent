//! Execution analysis domain: execution graph views, path / decision /
//! probability analysis and per-node iteration analysis. Handlers are thin
//! transport adapters over the `wf-api::workflow` analysis surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use crate::envelope::{err, error_response, ok};
use crate::extract::{IdNodePath, IdPath};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── execution graph ──
        .route("/executions/{id}/graph", get(handle_execution_graph))
        .route(
            "/executions/{id}/graph/nodes",
            get(handle_execution_graph_nodes),
        )
        .route(
            "/executions/{id}/graph/edges",
            get(handle_execution_graph_edges),
        )
        .route(
            "/executions/{id}/graph/neighbors/{nodeId}",
            get(handle_execution_graph_neighbors),
        )
        .route(
            "/executions/{id}/graph/path-stats",
            get(handle_execution_path_stats),
        )
        .route(
            "/executions/{id}/graph/reachability",
            get(handle_execution_graph_reachability),
        )
        .route(
            "/executions/{id}/graph/clear",
            post(handle_clear_execution_graph),
        )
        .route(
            "/executions/{id}/analysis/paths",
            get(handle_analysis_paths),
        )
        .route(
            "/executions/{id}/analysis/paths/enumerate",
            get(handle_enumerate_paths),
        )
        .route(
            "/executions/{id}/analysis/decision-points",
            get(handle_decision_points),
        )
        .route(
            "/executions/{id}/analysis/slow-nodes",
            get(handle_slow_nodes),
        )
        .route(
            "/executions/{id}/analysis/efficiency",
            get(handle_analysis_efficiency),
        )
        .route(
            "/executions/{id}/analysis/alternatives",
            get(handle_analysis_alternatives),
        )
        .route(
            "/executions/{id}/analysis/probabilities",
            get(handle_analysis_probabilities),
        )
        // ── iteration analysis ──
        .route("/executions/{id}/nodes", get(handle_execution_nodes))
        .route("/executions/{id}/nodes/{nodeId}", get(handle_node_analysis))
        .route(
            "/executions/{id}/nodes/by-type/{nodeType}",
            get(handle_nodes_by_type),
        )
        .route(
            "/executions/{id}/nodes/{nodeId}/input-context",
            get(handle_node_input_context),
        )
        .route(
            "/executions/{id}/nodes/{nodeId}/transitions",
            get(handle_node_transitions),
        )
        .route(
            "/executions/{id}/tool-chain/{nodeId}",
            get(handle_tool_chain),
        )
        .route("/executions/{id}/path", get(handle_execution_path))
        .route("/executions/{id}/optimizations", get(handle_optimizations))
        .route("/executions/{id}/node-stats", get(handle_node_stats))
        .route("/executions/{id}/failed-nodes", get(handle_failed_nodes))
        .route("/executions/{id}/iterations", get(handle_iterations))
        .route(
            "/executions/{id}/llm-reasoning-path/{nodeId}",
            get(handle_llm_reasoning_path),
        )
}

// ── execution graph ───────────────────────────────────────────────

async fn handle_execution_graph(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::get_execution_graph(&state.ctx, &path.id).await {
        Ok(graph) => ok(graph).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_execution_graph_nodes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::execution_graph_nodes(&state.ctx, &path.id).await {
        Ok(nodes) => ok(nodes).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_execution_graph_edges(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::execution_graph_edges(&state.ctx, &path.id).await {
        Ok(edges) => ok(edges).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_execution_graph_neighbors(
    State(state): State<ApiState>,
    Path(path): Path<IdNodePath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::execution_graph_node_neighbors(
        &state.ctx,
        &path.id,
        &path.node_id,
    )
    .await
    {
        Ok(neighbors) => ok(neighbors).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_execution_path_stats(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::get_execution_path_statistics(&state.ctx, &path.id).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

// ── execution graph analysis ──────────────────────────────────────

async fn handle_analysis_paths(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_graph::analyze(&state.ctx, &path.id).await {
        Ok(analysis) => ok(analysis).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_execution_graph_reachability(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::get_execution_graph(&state.ctx, &path.id).await {
        Ok(graph) => {
            let reachable = wf_api::workflow::execution_graph::reachable_nodes(&graph);
            ok(reachable).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_clear_execution_graph(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_graph::clear_execution_data(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_enumerate_paths(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::get_execution_graph(&state.ctx, &path.id).await {
        Ok(graph) => {
            let paths = wf_api::workflow::execution_graph::enumerate_paths(&graph);
            ok(paths).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_decision_points(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    let analysis = wf_api::workflow::execution_graph::analyze(&state.ctx, &path.id).await;
    match analysis {
        Ok(analysis) => ok(analysis.decision_points).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SlowNodesQuery {
    percentile: Option<f64>,
}

async fn handle_slow_nodes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<SlowNodesQuery>,
) -> impl IntoResponse {
    let percentile = match query.percentile {
        Some(value) if (0.0..=100.0).contains(&value) => value,
        Some(_) => {
            return err::<serde_json::Value>(crate::envelope::ApiError::validation(
                "percentile must be within 0..=100",
            ))
            .into_response()
        }
        None => 95.0,
    };
    match wf_api::workflow::execution_graph::get_slow_nodes(&state.ctx, &path.id, percentile).await
    {
        Ok(nodes) => ok(nodes).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_analysis_efficiency(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_graph::analyze_efficiency(&state.ctx, &path.id).await {
        Ok(efficiency) => ok(efficiency).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_analysis_alternatives(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_graph::get_alternative_paths(&state.ctx, &path.id).await {
        Ok(alternatives) => ok(alternatives).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_analysis_probabilities(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_graph::get_path_probability_analysis(&state.ctx, &path.id)
        .await
    {
        Ok(probabilities) => ok(probabilities).into_response(),
        Err(e) => error_response(e),
    }
}

// ── iteration analysis ────────────────────────────────────────────

async fn handle_execution_nodes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_iteration::get_execution_node_analyses(&state.ctx, &path.id)
        .await
    {
        Ok(nodes) => ok(nodes).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_node_analysis(
    State(state): State<ApiState>,
    Path(path): Path<IdNodePath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_iteration::get_node_analysis(
        &state.ctx,
        &path.id,
        &path.node_id,
    )
    .await
    {
        Ok(node) => ok(node).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NodeTypePath {
    id: String,
    node_type: String,
}

async fn handle_nodes_by_type(
    State(state): State<ApiState>,
    Path(path): Path<NodeTypePath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_iteration::get_node_executions_by_type(
        &state.ctx,
        &path.id,
        &path.node_type,
    )
    .await
    {
        Ok(nodes) => ok(nodes).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_node_input_context(
    State(state): State<ApiState>,
    Path(path): Path<IdNodePath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_node_input_context(
        &state.ctx,
        &path.id,
        &path.node_id,
    )
    .await
    {
        Ok(context) => ok(context).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct NodeTransitionsQuery {
    from: Option<String>,
    to: Option<String>,
}

async fn handle_node_transitions(
    State(state): State<ApiState>,
    Path(path): Path<IdNodePath>,
    Query(query): Query<NodeTransitionsQuery>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_node_transitions(
        &state.ctx,
        &path.id,
        query.from.as_deref(),
        query.to.as_deref(),
    )
    .await
    {
        Ok(transitions) => ok(transitions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_llm_reasoning_path(
    State(state): State<ApiState>,
    Path(path): Path<IdNodePath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_iteration::get_llm_reasoning_path(
        &state.ctx,
        &path.id,
        &path.node_id,
    )
    .await
    {
        Ok(records) => ok(records).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_tool_chain(
    State(state): State<ApiState>,
    Path(path): Path<IdNodePath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_iteration::get_tool_dependency_chain(
        &state.ctx,
        &path.id,
        &path.node_id,
    )
    .await
    {
        Ok(chain) => ok(chain).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_execution_path(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_iteration::get_execution_path(&state.ctx, &path.id).await {
        Ok(path_view) => ok(path_view).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_optimizations(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_iteration::get_optimization_opportunities(&state.ctx, &path.id)
        .await
    {
        Ok(opportunities) => ok(opportunities).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct NodeStatsQuery {
    node_id: Option<String>,
    node_type: Option<String>,
    status: Option<String>,
    has_errors: Option<bool>,
    min_duration: Option<i64>,
}

async fn handle_node_stats(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<NodeStatsQuery>,
) -> impl IntoResponse {
    let filter = wf_api::workflow::workflow_iteration::ExtendedNodeExecutionFilter {
        execution_ids: None,
        node_id: query.node_id,
        node_type: query.node_type,
        status: query.status,
        has_errors: query.has_errors.unwrap_or(false),
        min_duration: query.min_duration,
        time_range: None,
    };
    match wf_api::workflow::workflow_iteration::get_node_execution_stats(
        &state.ctx,
        &path.id,
        Some(&filter),
    )
    .await
    {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_failed_nodes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::workflow_iteration::get_failed_nodes(&state.ctx, &path.id).await {
        Ok(nodes) => ok(nodes).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_iterations(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::iteration::analyze(&state.ctx, &path.id).await {
        Ok(analysis) => ok(analysis).into_response(),
        Err(e) => error_response(e),
    }
}

//! Workflow graph query subface: graph / summary / nodes / edges /
//! neighbors / analysis / cycles / topology / reachability. Split from
//! `api_workflows` to keep the workflow surface at a maintainable size.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{error_response, ok};
use crate::extract::{IdNodePath, IdPath};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/workflows/{id}/graph", get(handle_graph))
        .route("/workflows/{id}/graph/summary", get(handle_graph_summary))
        .route("/workflows/{id}/graph/nodes", get(handle_graph_nodes))
        .route("/workflows/{id}/graph/edges", get(handle_graph_edges))
        .route(
            "/workflows/{id}/graph/neighbors/{nodeId}",
            get(handle_graph_neighbors),
        )
        .route("/workflows/{id}/graph/analysis", get(handle_graph_analysis))
        .route("/workflows/{id}/graph/cycles", get(handle_graph_cycles))
        .route("/workflows/{id}/graph/topology", get(handle_graph_topology))
        .route(
            "/workflows/{id}/graph/reachability",
            get(handle_graph_reachability),
        )
}

// ── typed view docs (C1 mirrors) ────────────────────────────────────
// Mirror the `wf-api` graph views and `wf-workflow` analysis results so
// codegen produces real types instead of `unknown`. The full graph
// structure (`WorkflowGraphStructure`, a foundation type with flattened
// node payloads) intentionally stays free-form: layout is frontend-owned
// and the backend never stores coordinates.

/// One workflow graph node: pure topology, no coordinates.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct GraphNodeDoc {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    node_type: String,
}

impl From<wf_api::workflow::graph_query::GraphNodeView> for GraphNodeDoc {
    fn from(view: wf_api::workflow::graph_query::GraphNodeView) -> Self {
        Self {
            id: view.id,
            name: view.name,
            node_type: view.node_type,
        }
    }
}

/// One workflow graph edge: endpoint ids plus edge type.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct GraphEdgeDoc {
    id: String,
    source_node_id: String,
    target_node_id: String,
    edge_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<String>,
}

impl From<wf_api::workflow::graph_query::GraphEdgeView> for GraphEdgeDoc {
    fn from(view: wf_api::workflow::graph_query::GraphEdgeView) -> Self {
        Self {
            id: view.id,
            source_node_id: view.source_node_id,
            target_node_id: view.target_node_id,
            edge_type: view.edge_type,
            condition: view.condition,
        }
    }
}

/// Aggregate summary of a workflow graph.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct GraphSummaryDoc {
    workflow_id: String,
    node_count: usize,
    edge_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_node_id: Option<String>,
    end_node_ids: Vec<String>,
    node_counts_by_type: BTreeMap<String, usize>,
}

impl From<wf_api::workflow::graph_query::GraphSummary> for GraphSummaryDoc {
    fn from(view: wf_api::workflow::graph_query::GraphSummary) -> Self {
        Self {
            workflow_id: view.workflow_id,
            node_count: view.node_count,
            edge_count: view.edge_count,
            start_node_id: view.start_node_id,
            end_node_ids: view.end_node_ids,
            node_counts_by_type: view.node_counts_by_type,
        }
    }
}

/// Predecessors and successors of one graph node.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct GraphNeighborsDoc {
    node_id: String,
    predecessors: Vec<String>,
    successors: Vec<String>,
}

impl From<wf_api::workflow::graph_query::GraphNeighborsView> for GraphNeighborsDoc {
    fn from(view: wf_api::workflow::graph_query::GraphNeighborsView) -> Self {
        Self {
            node_id: view.node_id,
            predecessors: view.predecessors,
            successors: view.successors,
        }
    }
}

/// Structural cycle detection result.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct CycleDetectionDoc {
    has_cycle: bool,
    cycle_nodes: Vec<String>,
    cycle_edges: Vec<String>,
}

/// Topological sort result.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct TopologicalSortDoc {
    success: bool,
    sorted_nodes: Vec<String>,
    cycle_nodes: Vec<String>,
}

/// Reachability analysis result (sets rendered as sorted arrays).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ReachabilityDoc {
    reachable_from_start: Vec<String>,
    reachable_to_end: Vec<String>,
    unreachable_nodes: Vec<String>,
    dead_end_nodes: Vec<String>,
}

/// Combined structural analysis of a workflow graph.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct GraphAnalysisDoc {
    cycle_detection: CycleDetectionDoc,
    topological_sort: TopologicalSortDoc,
    reachability: ReachabilityDoc,
    node_total: usize,
    edge_total: usize,
    node_counts_by_type: BTreeMap<String, usize>,
}

/// Digest of one resolved execution path.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ExecutionPathStatsDoc {
    node_count: usize,
    edge_count: usize,
    nodes: Vec<String>,
}

impl From<wf_api::workflow::graph_query::ExecutionPathStatsView> for ExecutionPathStatsDoc {
    fn from(view: wf_api::workflow::graph_query::ExecutionPathStatsView) -> Self {
        Self {
            node_count: view.node_count,
            edge_count: view.edge_count,
            nodes: view.nodes,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Workflow graph: pure topology (nodes/edges/adjacency, no coordinates; layout is frontend-owned)", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::get_graph(&state.ctx, &path.id).await {
        Ok(graph) => ok(graph).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph/summary",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Graph summary", body = crate::envelope::ApiEnvelope<GraphSummaryDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_summary(&state.ctx, &path.id).await {
        Ok(summary) => ok(GraphSummaryDoc::from(summary)).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct GraphNodesQuery {
    node_type: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph/nodes",
    tag = "workflow",
    params(IdPath, GraphNodesQuery),
    responses((status = 200, description = "Graph nodes", body = crate::envelope::ApiEnvelope<Vec<GraphNodeDoc>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_nodes(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<GraphNodesQuery>,
) -> impl IntoResponse {
    let result = match query.node_type {
        Some(node_type) => {
            wf_api::workflow::graph_query::graph_nodes_by_type(&state.ctx, &path.id, &node_type)
                .await
        }
        None => wf_api::workflow::graph_query::graph_nodes(&state.ctx, &path.id).await,
    };
    match result {
        Ok(nodes) => ok(nodes
            .into_iter()
            .map(GraphNodeDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph/edges",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Graph edges", body = crate::envelope::ApiEnvelope<Vec<GraphEdgeDoc>>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_edges(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_edges(&state.ctx, &path.id).await {
        Ok(edges) => ok(edges
            .into_iter()
            .map(GraphEdgeDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph/neighbors/{nodeId}",
    tag = "workflow",
    params(IdNodePath),
    responses((status = 200, description = "Node neighbors", body = crate::envelope::ApiEnvelope<GraphNeighborsDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_neighbors(
    State(state): State<ApiState>,
    Path(path): Path<IdNodePath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_node_neighbors(&state.ctx, &path.id, &path.node_id)
        .await
    {
        Ok(neighbors) => ok(GraphNeighborsDoc::from(neighbors)).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph/analysis",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Combined graph analysis", body = crate::envelope::ApiEnvelope<GraphAnalysisDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_analysis(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_analysis(&state.ctx, &path.id).await {
        Ok(analysis) => {
            let mut reachable_from_start: Vec<String> = analysis
                .reachability
                .reachable_from_start
                .into_iter()
                .collect();
            reachable_from_start.sort();
            let mut reachable_to_end: Vec<String> =
                analysis.reachability.reachable_to_end.into_iter().collect();
            reachable_to_end.sort();
            ok(GraphAnalysisDoc {
                cycle_detection: CycleDetectionDoc {
                    has_cycle: analysis.cycle_detection.has_cycle,
                    cycle_nodes: analysis.cycle_detection.cycle_nodes,
                    cycle_edges: analysis.cycle_detection.cycle_edges,
                },
                topological_sort: TopologicalSortDoc {
                    success: analysis.topological_sort.success,
                    sorted_nodes: analysis.topological_sort.sorted_nodes,
                    cycle_nodes: analysis.topological_sort.cycle_nodes,
                },
                reachability: ReachabilityDoc {
                    reachable_from_start,
                    reachable_to_end,
                    unreachable_nodes: analysis.reachability.unreachable_nodes,
                    dead_end_nodes: analysis.reachability.dead_end_nodes,
                },
                node_total: analysis.node_total,
                edge_total: analysis.edge_total,
                node_counts_by_type: analysis.node_counts_by_type.into_iter().collect(),
            })
            .into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph/cycles",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Cycle detection", body = crate::envelope::ApiEnvelope<CycleDetectionDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_cycles(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_detect_cycles(&state.ctx, &path.id).await {
        Ok(cycles) => ok(CycleDetectionDoc {
            has_cycle: cycles.has_cycle,
            cycle_nodes: cycles.cycle_nodes,
            cycle_edges: cycles.cycle_edges,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph/topology",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Topological sort", body = crate::envelope::ApiEnvelope<TopologicalSortDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_topology(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_topological_sort(&state.ctx, &path.id).await {
        Ok(sort) => ok(TopologicalSortDoc {
            success: sort.success,
            sorted_nodes: sort.sorted_nodes,
            cycle_nodes: sort.cycle_nodes,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/graph/reachability",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Reachability analysis", body = crate::envelope::ApiEnvelope<ReachabilityDoc>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_graph_reachability(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_reachability(&state.ctx, &path.id).await {
        Ok(reachability) => {
            let mut reachable_from_start: Vec<String> =
                reachability.reachable_from_start.into_iter().collect();
            reachable_from_start.sort();
            let mut reachable_to_end: Vec<String> =
                reachability.reachable_to_end.into_iter().collect();
            reachable_to_end.sort();
            ok(ReachabilityDoc {
                reachable_from_start,
                reachable_to_end,
                unreachable_nodes: reachability.unreachable_nodes,
                dead_end_nodes: reachability.dead_end_nodes,
            })
            .into_response()
        }
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::Request;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    fn sample_workflow(id: &str) -> wf_types::WorkflowDefinition {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": format!("Workflow {id}"),
            "version": "1.0.0",
            "nodes": [
                {"id": "start", "node_type": "START", "name": "start"},
                {"id": "end", "node_type": "END", "name": "end"}
            ],
            "edges": [
                {"id": "e1", "source_node_id": "start", "target_node_id": "end", "type": "DEFAULT"}
            ],
            "created_at": 1000,
            "updated_at": 1000
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn workflow_graph_endpoints_work() {
        let ctx = make_ctx();
        let create = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows")
                    .header("content-type", "application/json")
                    .body(AxBody::from(
                        serde_json::to_vec(&sample_workflow("wf-graph")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create.status(), axum::http::StatusCode::OK);

        for uri in [
            "/api/v1/workflows/wf-graph/graph",
            "/api/v1/workflows/wf-graph/graph/summary",
            "/api/v1/workflows/wf-graph/graph/nodes",
            "/api/v1/workflows/wf-graph/graph/edges",
            "/api/v1/workflows/wf-graph/graph/neighbors/start",
            "/api/v1/workflows/wf-graph/graph/analysis",
            "/api/v1/workflows/wf-graph/graph/cycles",
            "/api/v1/workflows/wf-graph/graph/topology",
            "/api/v1/workflows/wf-graph/graph/reachability",
        ] {
            let response = crate::router::api_router(ctx.clone())
                .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK, "uri: {uri}");
        }
    }
}

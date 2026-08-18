//! Workflow graph query subface: graph / summary / nodes / edges /
//! neighbors / analysis / cycles / topology / reachability. Split from
//! `api_workflows` to keep the workflow surface at a maintainable size.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

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

async fn handle_graph(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::get_graph(&state.ctx, &path.id).await {
        Ok(graph) => ok(graph).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_graph_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_summary(&state.ctx, &path.id).await {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct GraphNodesQuery {
    node_type: Option<String>,
}

async fn handle_graph_nodes(
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
        Ok(nodes) => ok(nodes).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_graph_edges(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_edges(&state.ctx, &path.id).await {
        Ok(edges) => ok(edges).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_graph_neighbors(
    State(state): State<ApiState>,
    Path(path): Path<IdNodePath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_node_neighbors(&state.ctx, &path.id, &path.node_id)
        .await
    {
        Ok(neighbors) => ok(neighbors).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_graph_analysis(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_analysis(&state.ctx, &path.id).await {
        Ok(analysis) => ok(serde_json::json!({
            "cycle_detection": {
                "has_cycle": analysis.cycle_detection.has_cycle,
                "cycle_nodes": analysis.cycle_detection.cycle_nodes,
                "cycle_edges": analysis.cycle_detection.cycle_edges,
            },
            "topological_sort": {
                "success": analysis.topological_sort.success,
                "sorted_nodes": analysis.topological_sort.sorted_nodes,
                "cycle_nodes": analysis.topological_sort.cycle_nodes,
            },
            "reachability": {
                "reachable_from_start": analysis.reachability.reachable_from_start,
                "reachable_to_end": analysis.reachability.reachable_to_end,
                "unreachable_nodes": analysis.reachability.unreachable_nodes,
                "dead_end_nodes": analysis.reachability.dead_end_nodes,
            },
            "node_total": analysis.node_total,
            "edge_total": analysis.edge_total,
            "node_counts_by_type": analysis.node_counts_by_type,
        }))
        .into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_graph_cycles(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_detect_cycles(&state.ctx, &path.id).await {
        Ok(cycles) => ok(serde_json::json!({
            "has_cycle": cycles.has_cycle,
            "cycle_nodes": cycles.cycle_nodes,
            "cycle_edges": cycles.cycle_edges,
        }))
        .into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_graph_topology(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_topological_sort(&state.ctx, &path.id).await {
        Ok(sort) => ok(serde_json::json!({
            "success": sort.success,
            "sorted_nodes": sort.sorted_nodes,
            "cycle_nodes": sort.cycle_nodes,
        }))
        .into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_graph_reachability(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::graph_query::graph_reachability(&state.ctx, &path.id).await {
        Ok(reachability) => ok(serde_json::json!({
            "reachable_from_start": reachability.reachable_from_start,
            "reachable_to_end": reachability.reachable_to_end,
            "unreachable_nodes": reachability.unreachable_nodes,
            "dead_end_nodes": reachability.dead_end_nodes,
        }))
        .into_response(),
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
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
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

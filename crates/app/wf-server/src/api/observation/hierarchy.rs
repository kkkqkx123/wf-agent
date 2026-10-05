//! Execution hierarchy domain: where one execution sits in the parent/child
//! tree of nested workflow and agent runs. Sibling of `history`, which
//! answers what a single execution recorded rather than how it relates to
//! others.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use wf_api::execution_hierarchy;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/executions/{id}/hierarchy", get(handle_hierarchy))
        .route("/executions/{id}/subtree", get(handle_subtree))
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/hierarchy",
    operation_id = "get_executions_id_hierarchy",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_hierarchy(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match execution_hierarchy::hierarchy(&state.ctx, &path.id).await {
        Ok(view) => ok(view).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/subtree",
    operation_id = "get_executions_id_subtree",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_subtree(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match execution_hierarchy::subtree(&state.ctx, &path.id).await {
        Ok(tree) => ok(tree).into_response(),
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
    use wf_storage::adapter::base::BaseStorageAdapter;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    async fn save_agent(ctx: &ApiContext, id: &str) {
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from(id.to_string()),
            definition_id: wf_types::Id::from("agent-1".to_string()),
            status: wf_types::ExecutionStatus::Completed,
            current_iteration: 1,
            tool_call_count: 0,
            iteration_history: None,
            started_at: 1,
            completed_at: Some(2),
            error: None,
            context: None,
            loop_config: None,
            permanently_failed_tools: None,
            hierarchy: None,
        };
        ctx.storage.agent_execution.save(&record).await.unwrap();
    }

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
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
    async fn hierarchy_routes_answer_persisted_executions() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-1").await;

        let response = send(ctx.clone(), "/api/v1/executions/loop-1/hierarchy").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["execution_id"], "loop-1");
        assert_eq!(body["data"]["execution_type"], "agent_loop");
        assert_eq!(body["data"]["depth"], 0);

        let tree = send(ctx.clone(), "/api/v1/executions/loop-1/subtree").await;
        assert_eq!(tree.status(), StatusCode::OK);
        let tree_body = json_body(tree).await;
        assert_eq!(tree_body["data"]["root_execution_id"], "loop-1");
        assert_eq!(tree_body["data"]["truncated"], false);
        assert_eq!(tree_body["data"]["nodes"].as_array().unwrap().len(), 1);

    }

    #[tokio::test]
    async fn hierarchy_routes_map_unknown_execution_to_not_found() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/executions/missing/hierarchy",
            "/api/v1/executions/missing/subtree",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
    }
}

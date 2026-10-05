//! Execution history domain: everything one execution recorded, grouped by
//! section. One endpoint covers both engines, so a consumer reads the same
//! shape for a workflow run and an agent loop; sections the owning engine
//! does not record come back empty rather than absent.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use utoipa::IntoParams;

use wf_api::execution_history::{self, ExecutionHistorySections};

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/executions/{id}/history", get(handle_history))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct HistoryQuery {
    /// Comma-separated sections to load; omit for all of
    /// `timeline,nodes,iterations,variables,context,transitions`.
    include: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/history",
    operation_id = "get_executions_id_history",
    tag = "observation",
    params(IdPath, HistoryQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_history(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
    let sections = match ExecutionHistorySections::parse(query.include.as_deref()) {
        Ok(sections) => sections,
        Err(e) => return error_response(e),
    };
    match execution_history::history(&state.ctx, &path.id, &sections).await {
        Ok(view) => ok(view).into_response(),
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
            iteration_history: Some(vec![wf_types::agent_execution::IterationRecord {
                iteration: 1,
                started_at: 10,
                completed_at: Some(20),
                tool_calls: None,
                response_content: None,
                llm_calls: None,
                error: None,
            }]),
            started_at: 1,
            completed_at: Some(30),
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
    async fn history_answers_agent_loop_iterations() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-h").await;

        let response = send(ctx.clone(), "/api/v1/executions/loop-h/history").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["execution_id"], "loop-h");
        assert_eq!(body["data"]["execution_type"], "agent_loop");
        assert_eq!(body["data"]["iterations"].as_array().unwrap().len(), 1);
        // Workflow-only sections are omitted rather than reported as empty.
        assert!(body["data"].get("node_executions").is_none());
    }

    #[tokio::test]
    async fn include_narrows_the_loaded_sections() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-i").await;

        let response = send(ctx, "/api/v1/executions/loop-i/history?include=timeline").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert!(body["data"].get("iterations").is_none());
    }

    #[tokio::test]
    async fn unknown_section_is_rejected() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-bad").await;

        let response = send(ctx, "/api/v1/executions/loop-bad/history?include=bogus").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let response = send(ctx, "/api/v1/executions/missing/history").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

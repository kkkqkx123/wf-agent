//! Execution bundle export: one versioned archive per execution for
//! disaster-recovery review and manual reproduction. The handler is a thin
//! transport adapter over `wf_api::execution_bundle::export_bundle`; the CLI
//! pull path reads this route instead of aggregating the history, artifact
//! and audit endpoints on its own.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/executions/{id}/bundle", get(handle_execution_bundle))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ExecutionBundleQuery {
    /// Mark the exported bundle as checkpoint-synced.
    checkpoint_synced: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/bundle",
    operation_id = "get_executions_id_bundle",
    tag = "observation",
    params(IdPath, ExecutionBundleQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execution_bundle(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ExecutionBundleQuery>,
) -> impl IntoResponse {
    match wf_api::execution_bundle::export_bundle(
        &state.ctx,
        &path.id,
        query.checkpoint_synced.unwrap_or(false),
    )
    .await
    {
        Ok(bundle) => ok(bundle).into_response(),
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
            execution_type: wf_types::execution::ExecutionType::Agent,
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

            rerun_of: None,
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
    async fn bundle_exports_a_known_execution() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-bundle").await;

        let response = send(ctx, "/api/v1/executions/loop-bundle/bundle").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["version"], 1);
        assert_eq!(body["data"]["execution_id"], "loop-bundle");
        assert_eq!(body["data"]["checkpoint_synced"], false);
    }

    #[tokio::test]
    async fn bundle_flag_marks_checkpoint_synced() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-synced").await;

        let response = send(
            ctx,
            "/api/v1/executions/loop-synced/bundle?checkpoint_synced=true",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["checkpoint_synced"], true);
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let response = send(ctx, "/api/v1/executions/missing/bundle").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

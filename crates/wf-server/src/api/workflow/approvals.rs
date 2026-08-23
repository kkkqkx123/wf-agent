//! Approval and user interaction surface: blocking tool-approval flows and
//! the persisted interaction records they produce.
//!
//! `/approvals/*` endpoints wait on a human responder without a wait
//! bound. When no `UserInteractionHandler` is registered there is nobody
//! to answer, so the endpoints fail fast instead of blocking forever.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_storage::adapter::user_interaction::UserInteractionListOptions;
use wf_types::interaction::tool_approval::ToolApprovalRequestData;
use wf_types::tool::{ToolApprovalOptions, ToolExecutionOptions};
use wf_types::UserInteractionStorageMetadata;

use crate::envelope::{err, error_response, ok, ApiError};
use crate::extract::{ExecutionIdPath, IdPath, ListQuery};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/approvals/request", post(handle_request_approval))
        .route("/approvals/check", post(handle_check_approval))
        .route("/approvals/execute-tool", post(handle_execute_tool))
        .route(
            "/interactions",
            get(handle_list_interactions).post(handle_save_interaction),
        )
        .route(
            "/interactions/by-execution/{executionId}",
            get(handle_interactions_by_execution),
        )
        .route(
            "/interactions/by-status/{status}",
            get(handle_interactions_by_status),
        )
        .route(
            "/interactions/{id}",
            get(handle_get_interaction).delete(handle_delete_interaction),
        )
        .route(
            "/interactions/{id}/respond",
            post(handle_respond_interaction),
        )
        .route("/interactions/stats", get(handle_interaction_stats))
}

#[derive(Deserialize)]
struct ApprovalRequestBody {
    execution_id: String,
    request: ToolApprovalRequestData,
}

async fn handle_request_approval(
    State(state): State<ApiState>,
    Json(body): Json<ApprovalRequestBody>,
) -> impl IntoResponse {
    if !wf_api::entity::user_interaction::has_handler(&state.ctx).await {
        return err::<Value>(ApiError::validation(
            "no user interaction handler is registered; cannot wait for approval",
        ))
        .into_response();
    }
    match wf_api::workflow::approval::request_user_approval(&state.ctx, &body.execution_id, &body.request)
        .await
    {
        Ok((interaction_id, response)) => ok((interaction_id, response)).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ApprovalCheckBody {
    execution_id: String,
    request: ToolApprovalRequestData,
    options: Option<ToolApprovalOptions>,
}

async fn handle_check_approval(
    State(state): State<ApiState>,
    Json(body): Json<ApprovalCheckBody>,
) -> impl IntoResponse {
    if !wf_api::entity::user_interaction::has_handler(&state.ctx).await {
        return err::<Value>(ApiError::validation(
            "no user interaction handler is registered; cannot wait for approval",
        ))
        .into_response();
    }
    match wf_api::workflow::approval::check_and_request_approval(
        &state.ctx,
        &body.execution_id,
        &body.request,
        body.options,
    )
    .await
    {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ExecuteToolBody {
    execution_id: String,
    tool_id: String,
    parameters: Value,
    options: Option<ToolExecutionOptions>,
    approval_options: Option<ToolApprovalOptions>,
}

async fn handle_execute_tool(
    State(state): State<ApiState>,
    Json(body): Json<ExecuteToolBody>,
) -> impl IntoResponse {
    if !wf_api::entity::user_interaction::has_handler(&state.ctx).await {
        return err::<Value>(ApiError::validation(
            "no user interaction handler is registered; cannot wait for approval",
        ))
        .into_response();
    }
    match wf_api::workflow::approval::execute_tool_with_approval(
        &state.ctx,
        &body.execution_id,
        &body.tool_id,
        &body.parameters,
        body.options,
        body.approval_options,
    )
    .await
    {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ListInteractionsQuery {
    #[serde(flatten)]
    page: ListQuery,
    execution_id: Option<String>,
    status: Option<String>,
    interaction_type: Option<String>,
}

async fn handle_list_interactions(
    State(state): State<ApiState>,
    Query(query): Query<ListInteractionsQuery>,
) -> impl IntoResponse {
    let options = UserInteractionListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        execution_id_filter: query.execution_id,
        status_filter: query.status,
        interaction_type_filter: query.interaction_type,
    };
    match wf_api::entity::user_interaction::list_interactions(&state.ctx.storage, Some(options))
        .await
    {
        Ok(interactions) => ok(interactions).into_response(),
        Err(e) => error_response(e),
    }
}

/// Persist an interaction record directly (approval flows create records
/// implicitly; this endpoint supports manual bookkeeping).
async fn handle_save_interaction(
    State(state): State<ApiState>,
    Json(interaction): Json<UserInteractionStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::entity::user_interaction::save_interaction(&state.ctx.storage, &interaction).await
    {
        Ok(()) => ok(interaction.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_interaction(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::user_interaction::get_interaction(&state.ctx.storage, &path.id).await {
        Ok(interaction) => ok(interaction).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_interaction(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::entity::user_interaction::delete_interaction(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_interactions_by_execution(
    State(state): State<ApiState>,
    Path(path): Path<ExecutionIdPath>,
) -> impl IntoResponse {
    match wf_api::entity::user_interaction::list_interactions_by_execution(
        &state.ctx.storage,
        &path.execution_id,
    )
    .await
    {
        Ok(interactions) => ok(interactions).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct StatusPath {
    status: String,
}

async fn handle_interactions_by_status(
    State(state): State<ApiState>,
    Path(path): Path<StatusPath>,
) -> impl IntoResponse {
    match wf_api::entity::user_interaction::list_interactions_by_status(
        &state.ctx.storage,
        &path.status,
    )
    .await
    {
        Ok(interactions) => ok(interactions).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct RespondBody {
    response_data: Option<Value>,
    result_data: Option<Value>,
}

async fn handle_respond_interaction(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<RespondBody>,
) -> impl IntoResponse {
    match wf_api::entity::user_interaction::respond_interaction(
        &state.ctx.storage,
        &path.id,
        body.response_data,
        body.result_data,
    )
    .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_interaction_stats(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::user_interaction::get_interaction_stats(&state.ctx.storage).await {
        Ok(stats) => ok(stats).into_response(),
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

    async fn send(ctx: Arc<ApiContext>, method: &str, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(AxBody::empty())
                    .unwrap(),
            )
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
    async fn approval_fails_fast_without_handler() {
        let ctx = make_ctx();
        let response = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/approvals/request")
                    .header("content-type", "application/json")
                    .body(AxBody::from(
                        serde_json::to_vec(&serde_json::json!({
                            "execution_id": "exec-1",
                            "request": {
                                "tool_call_id": "call-1",
                                "tool_name": "bash",
                                "parameters": {"command": "ls"}
                            }
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn interactions_are_queryable() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/interactions",
            "/api/v1/interactions/stats",
            "/api/v1/interactions/by-execution/exec-1",
            "/api/v1/interactions/by-status/pending",
        ] {
            let response = send(ctx.clone(), "GET", uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }

        let missing = send(ctx.clone(), "GET", "/api/v1/interactions/nope").await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let body = json_body(missing).await;
        assert_eq!(body["success"], false);

        let deleted = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/interactions/nope")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(deleted.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn interaction_can_be_saved_directly() {
        let ctx = make_ctx();
        let interaction = serde_json::json!({
            "id": "inter-1",
            "execution_id": "exec-1",
            "interaction_type": "manual",
            "status": "pending",
            "request_data": {"question": "proceed?"},
            "created_at": 1000
        });
        let saved = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/interactions")
                    .header("content-type", "application/json")
                    .body(AxBody::from(interaction.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(saved.status(), StatusCode::OK, "save interaction");

        let fetched = send(ctx.clone(), "GET", "/api/v1/interactions/inter-1").await;
        assert_eq!(
            fetched.status(),
            StatusCode::OK,
            "fetched saved interaction"
        );
    }
}

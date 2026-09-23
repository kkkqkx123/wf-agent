//! Batch operations: per-id control over many executions, loops and
//! interactions in one round trip. Results are per item (`ok` plus an error
//! message on failure) so partial success is explicit instead of silent.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/executions/batch-cancel", post(handle_batch_cancel))
        .route("/executions/batch-delete", post(handle_batch_delete))
        .route("/agent-loops/batch-delete", post(handle_batch_delete_loops))
        .route("/interactions/batch-respond", post(handle_batch_respond))
}

/// Maximum ids per batch; bounds one request's fan-out.
const MAX_BATCH_IDS: usize = 100;

#[derive(Deserialize, ToSchema)]
pub(crate) struct IdsBody {
    ids: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct BatchRespondBody {
    ids: Vec<String>,
    response_data: Option<serde_json::Value>,
    result_data: Option<serde_json::Value>,
    /// Optional loop ownership guard; when present each id is validated
    /// against this loop before responding.
    agent_loop_id: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct BatchItemResult {
    pub(crate) id: String,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

fn validate_ids(ids: &[String]) -> Result<(), wf_api::ApiError> {
    if ids.is_empty() {
        return Err(wf_api::ApiError::Validation(
            "ids must not be empty".to_string(),
        ));
    }
    if ids.len() > MAX_BATCH_IDS {
        return Err(wf_api::ApiError::Validation(format!(
            "at most {MAX_BATCH_IDS} ids per batch"
        )));
    }
    Ok(())
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/batch-cancel",
    tag = "web",
    request_body = IdsBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_batch_cancel(
    State(state): State<ApiState>,
    Json(body): Json<IdsBody>,
) -> impl IntoResponse {
    if let Err(e) = validate_ids(&body.ids) {
        return error_response(e);
    }
    let mut results = Vec::with_capacity(body.ids.len());
    for id in &body.ids {
        match wf_api::workflow::workflow_execution::cancel(&state.ctx, id).await {
            Ok(()) => results.push(BatchItemResult {
                id: id.clone(),
                ok: true,
                error: None,
            }),
            Err(e) => results.push(BatchItemResult {
                id: id.clone(),
                ok: false,
                error: Some(e.to_string()),
            }),
        }
    }
    ok(results).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/batch-delete",
    tag = "web",
    request_body = IdsBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_batch_delete(
    State(state): State<ApiState>,
    Json(body): Json<IdsBody>,
) -> impl IntoResponse {
    if let Err(e) = validate_ids(&body.ids) {
        return error_response(e);
    }
    let mut results = Vec::with_capacity(body.ids.len());
    for id in &body.ids {
        match wf_api::workflow::delete_execution(&state.ctx, id).await {
            Ok(_) => results.push(BatchItemResult {
                id: id.clone(),
                ok: true,
                error: None,
            }),
            Err(e) => results.push(BatchItemResult {
                id: id.clone(),
                ok: false,
                error: Some(e.to_string()),
            }),
        }
    }
    ok(results).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/batch-delete",
    tag = "web",
    request_body = IdsBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_batch_delete_loops(
    State(state): State<ApiState>,
    Json(body): Json<IdsBody>,
) -> impl IntoResponse {
    if let Err(e) = validate_ids(&body.ids) {
        return error_response(e);
    }
    let mut results = Vec::with_capacity(body.ids.len());
    for id in &body.ids {
        match wf_api::agent::agent::delete_agent_loop(&state.ctx.storage, id).await {
            Ok(_) => results.push(BatchItemResult {
                id: id.clone(),
                ok: true,
                error: None,
            }),
            Err(e) => results.push(BatchItemResult {
                id: id.clone(),
                ok: false,
                error: Some(e.to_string()),
            }),
        }
    }
    ok(results).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/interactions/batch-respond",
    tag = "web",
    request_body = BatchRespondBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_batch_respond(
    State(state): State<ApiState>,
    Json(body): Json<BatchRespondBody>,
) -> impl IntoResponse {
    if let Err(e) = validate_ids(&body.ids) {
        return error_response(e);
    }
    let mut results = Vec::with_capacity(body.ids.len());
    for id in &body.ids {
        let outcome = if let Some(loop_id) = body.agent_loop_id.as_deref() {
            wf_api::agent::agent_user_interaction::respond(
                &state.ctx,
                loop_id,
                id,
                body.response_data.clone(),
                body.result_data.clone(),
            )
            .await
        } else {
            wf_api::entity::user_interaction::respond_interaction(
                &state.ctx.storage,
                id,
                body.response_data.clone(),
                body.result_data.clone(),
            )
            .await
        };
        match outcome {
            Ok(()) => results.push(BatchItemResult {
                id: id.clone(),
                ok: true,
                error: None,
            }),
            Err(e) => results.push(BatchItemResult {
                id: id.clone(),
                ok: false,
                error: Some(e.to_string()),
            }),
        }
    }
    ok(results).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn post(ctx: Arc<ApiContext>, uri: &str, body: &str) -> axum::response::Response {
        crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(AxBody::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn batch_reports_per_item_failure() {
        let ctx = make_ctx();
        let response = post(
            ctx,
            "/api/v1/executions/batch-cancel",
            r#"{"ids":["missing"]}"#,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let items = body["data"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["ok"], false);
        assert!(items[0]["error"].is_string());
    }

    #[tokio::test]
    async fn batch_rejects_empty_and_oversize() {
        let ctx = make_ctx();
        let empty = post(
            ctx.clone(),
            "/api/v1/agent-loops/batch-delete",
            r#"{"ids":[]}"#,
        )
        .await;
        assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
        let many = format!(
            r#"{{"ids":[{}]}}"#,
            (0..101)
                .map(|i| format!("\"e{i}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        let oversize = post(ctx, "/api/v1/agent-loops/batch-delete", &many).await;
        assert_eq!(oversize.status(), StatusCode::BAD_REQUEST);
    }
}

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

#[derive(Deserialize, ToSchema)]
pub struct ChatTitleBody {
    pub session_id: String,
    pub profile_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct LoopTitleBody {
    #[serde(default)]
    pub profile_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct TitleResponse {
    pub text: Option<String>,
}

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/chat/title", post(handle_chat_title))
        .route("/agent-loops/{id}/title", post(handle_loop_title))
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/title",
    tag = "chat",
    request_body = ChatTitleBody,
    responses(
        (status = 200, description = "Title text or null when silent", body = crate::envelope::ApiEnvelope<TitleResponse>),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_chat_title(
    State(state): State<ApiState>,
    Json(body): Json<ChatTitleBody>,
) -> impl IntoResponse {
    let params = wf_api::title::TitleParams {
        session_id: body.session_id,
        profile_id: body.profile_id,
    };
    match wf_api::title::title(&state.ctx, params).await {
        Ok(text) => ok(TitleResponse { text }).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/title",
    tag = "agent",
    params(IdPath),
    request_body = LoopTitleBody,
    responses(
        (status = 200, description = "Title text or null when silent", body = crate::envelope::ApiEnvelope<TitleResponse>),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_title(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<LoopTitleBody>,
) -> impl IntoResponse {
    let params = wf_api::title::TitleParams {
        session_id: path.id,
        profile_id: body.profile_id,
    };
    match wf_api::title::title(&state.ctx, params).await {
        Ok(text) => ok(TitleResponse { text }).into_response(),
        Err(e) => error_response(e),
    }
}

//! Chat domain: the tool-free multi-turn chat executor.
//!
//! One endpoint runs one chat turn. The turn belongs to a session: the
//! session id anchors the conversation, so a session continues where the
//! previous turn left off and its transcript can be replayed.

pub(crate) mod starters;

use axum::extract::State;
use axum::response::Response;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::envelope::error_response;
use crate::router::ApiState;
use crate::sse::{execution_frames, sse_response};

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/chat/stream", post(handle_chat_stream))
        .merge(starters::routes())
}

/// Wire body of `/chat/stream`.
#[derive(Deserialize, ToSchema)]
pub struct ChatRunBody {
    /// LLM profile id that serves the turn.
    profile_id: String,
    /// The user text of this turn.
    message: String,
    /// Session anchor of the conversation. Absent starts a new session.
    #[serde(default)]
    session_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/stream",
    tag = "chat",
    request_body = ChatRunBody,
    responses(
        (status = 200, description = "Server-sent events stream", content_type = "text/event-stream"),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_chat_stream(
    State(state): State<ApiState>,
    Json(body): Json<ChatRunBody>,
) -> Response {
    let params = wf_api::chat::ChatTurnParams {
        profile_id: body.profile_id,
        message: body.message,
        session_id: body.session_id,
    };
    match wf_api::chat::stream(&state.ctx, params).await {
        Ok(stream) => sse_response(execution_frames(stream)),
        Err(e) => error_response(e),
    }
}

//! Screen-only recap reads: one short line recalling where a turn left
//! off. Both chat sessions and agent loops share the message store keyed by
//! session/loop id, so both routes call the same `wf-api::recap` face.
//!
//! The response is always a success envelope carrying `text`: `Some` shows
//! the line, `None` means silence (nothing worth saying, side call failed,
//! timed out or history empty). Only caller misuse (blank ids) reports an
//! error. No extra HTTP timeout is layered above the 25s side-call bound,
//! and in-progress turns are not rejected: the read is snapshot-only and
//! frontend throttling (at most once per turn) owns the rate.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

/// Wire body of `POST /chat/recap`.
#[derive(Deserialize, ToSchema)]
pub struct ChatRecapBody {
    /// Session anchor of the conversation.
    pub session_id: String,
    /// LLM profile id that ran the session and serves the side call.
    pub profile_id: String,
}

/// Wire body of `POST /agent-loops/{id}/recap`.
#[derive(Deserialize, ToSchema)]
pub struct LoopRecapBody {
    /// LLM profile id that ran the loop and serves the side call.
    #[serde(default)]
    pub profile_id: String,
}

/// Success payload of both recap routes.
#[derive(Serialize, ToSchema)]
pub struct RecapResponse {
    /// The cleaned recap line, or null when this turn has no recap.
    pub text: Option<String>,
}

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/chat/recap", post(handle_chat_recap))
        .route("/agent-loops/{id}/recap", post(handle_loop_recap))
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/recap",
    tag = "chat",
    request_body = ChatRecapBody,
    responses(
        (status = 200, description = "Recap line or null when silent", body = crate::envelope::ApiEnvelope<RecapResponse>),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_chat_recap(
    State(state): State<ApiState>,
    Json(body): Json<ChatRecapBody>,
) -> impl IntoResponse {
    let params = wf_api::recap::RecapParams {
        session_id: body.session_id,
        profile_id: body.profile_id,
    };
    match wf_api::recap::recap(&state.ctx, params).await {
        Ok(text) => ok(RecapResponse { text }).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-loops/{id}/recap",
    tag = "agent",
    params(IdPath),
    request_body = LoopRecapBody,
    responses(
        (status = 200, description = "Recap line or null when silent", body = crate::envelope::ApiEnvelope<RecapResponse>),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_loop_recap(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<LoopRecapBody>,
) -> impl IntoResponse {
    let params = wf_api::recap::RecapParams {
        session_id: path.id,
        profile_id: body.profile_id,
    };
    match wf_api::recap::recap(&state.ctx, params).await {
        Ok(text) => ok(RecapResponse { text }).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_ctx() -> Arc<wf_api::ApiContext> {
        Arc::new(wf_api::ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    #[tokio::test]
    async fn missing_history_is_silent_and_blank_ids_are_rejected() {
        let state = ApiState {
            ctx: make_ctx(),
            config: Arc::new(crate::middleware::ServerMiddlewareConfig::default()),
            locks: crate::api::workflow::locks::DefinitionLocks::default(),
        };
        // Unknown session: no history, so the read converges to null text
        // with a success envelope instead of an error bubble.
        let silent = handle_chat_recap(
            State(state.clone()),
            Json(ChatRecapBody {
                session_id: "missing".to_string(),
                profile_id: "mock".to_string(),
            }),
        )
        .await
        .into_response();
        assert_eq!(silent.status(), axum::http::StatusCode::OK);
        // Blank profile is caller misuse and reports a validation error.
        let invalid = handle_loop_recap(
            State(state),
            Path(IdPath {
                id: "session-1".to_string(),
            }),
            Json(LoopRecapBody {
                profile_id: String::new(),
            }),
        )
        .await
        .into_response();
        assert_eq!(invalid.status(), axum::http::StatusCode::BAD_REQUEST);
    }
}

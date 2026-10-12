use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/git/action", post(handle_git_action))
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct GitActionBody {
    action: String,
    #[serde(default)]
    params: std::collections::HashMap<String, String>,
    #[serde(default)]
    confirm: Option<String>,
}

#[utoipa::path(post, path = "/api/v1/git/action", operation_id = "post_git_action", tag = "git", request_body = GitActionBody, responses((status = 200, description = "Action outcome with command echo", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Unknown action or missing params", body = crate::envelope::ErrorResponse), (status = 409, description = "Unavailable or confirm required", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_git_action(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<GitActionBody>,
) -> impl IntoResponse {
    let request = wf_api::git::ActionRequest {
        action: body.action,
        params: body.params,
        confirm: body.confirm,
    };
    match wf_api::git::execute_action(&state.ctx, &request) {
        Ok(outcome) => ok(outcome).into_response(),
        Err(err) => error_response(err.into()),
    }
}

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/git/terminal", post(handle_create_terminal))
        .route("/git/terminal/{id}/input", post(handle_terminal_input))
        .route("/git/terminal/{id}/output", get(handle_terminal_output))
        .route("/git/terminal/{id}/resize", post(handle_terminal_resize))
        .route("/git/terminal/{id}/close", post(handle_close_terminal))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct TerminalOutputQuery {
    offset: Option<usize>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct TerminalInputBody {
    input: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct TerminalResizeBody {
    #[serde(default = "default_rows")]
    rows: u32,
    #[serde(default = "default_cols")]
    cols: u32,
}

fn default_rows() -> u32 {
    24
}

fn default_cols() -> u32 {
    80
}

#[utoipa::path(post, path = "/api/v1/git/terminal", operation_id = "post_git_terminal", tag = "git", responses((status = 200, description = "Terminal session created", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_create_terminal(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::git::create_terminal_session(&state.ctx) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(post, path = "/api/v1/git/terminal/{id}/input", operation_id = "post_git_terminal_input", tag = "git", params(IdPath), request_body = TerminalInputBody, responses((status = 200, description = "Input executed, output appended", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_terminal_input(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<TerminalInputBody>,
) -> impl IntoResponse {
    match wf_api::git::terminal_input(&state.ctx, &id, &body.input) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(get, path = "/api/v1/git/terminal/{id}/output", operation_id = "get_git_terminal_output", tag = "git", params(IdPath, TerminalOutputQuery), responses((status = 200, description = "Terminal output slice", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_terminal_output(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(query): Query<TerminalOutputQuery>,
) -> impl IntoResponse {
    match wf_api::git::terminal_output(&state.ctx, &id, query.offset.unwrap_or(0)) {
        Ok(text) => ok(serde_json::json!({"output": text})).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(post, path = "/api/v1/git/terminal/{id}/resize", operation_id = "post_git_terminal_resize", tag = "git", params(IdPath), request_body = TerminalResizeBody, responses((status = 200, description = "Terminal resized", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_terminal_resize(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<TerminalResizeBody>,
) -> impl IntoResponse {
    match wf_api::git::terminal_resize(&state.ctx, &id, body.rows, body.cols) {
        Ok(view) => ok(view).into_response(),
        Err(err) => error_response(err.into()),
    }
}

#[utoipa::path(post, path = "/api/v1/git/terminal/{id}/close", operation_id = "post_git_terminal_close", tag = "git", params(IdPath), responses((status = 200, description = "Terminal closed", body = crate::envelope::ApiEnvelope<serde_json::Value>)), security(("api_key" = [])))]
pub(crate) async fn handle_close_terminal(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match wf_api::git::close_terminal_session(&state.ctx, &id) {
        Ok(closed) => ok(serde_json::json!({"closed": closed})).into_response(),
        Err(err) => error_response(err.into()),
    }
}

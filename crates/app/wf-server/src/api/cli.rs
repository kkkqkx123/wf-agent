//! Read-only CLI snapshot surface: the versioned, data-only projection
//! of plugin CLI contributions (`cli-command`, `completion-provider`,
//! `setting-field`, `tool-summary`, `markdown-transformer`) that shells
//! merge with machine-local plugins. Carries no code and no secrets;
//! shells re-validate every entry and fail back to builtin-only behavior
//! when the server is plugin-less or unreachable.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::envelope::ok;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/cli/snapshot", get(handle_cli_snapshot))
}

#[utoipa::path(
    get,
    path = "/api/v1/cli/snapshot",
    tag = "cli",
    responses((status = 200, description = "Versioned CLI contribution snapshot; empty when the server runs no plugins", body = crate::envelope::ApiEnvelope<wf_types::cli::CliSnapshot>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_cli_snapshot(State(state): State<ApiState>) -> impl IntoResponse {
    ok(state.ctx.plugin_source.cli_snapshot()).into_response()
}

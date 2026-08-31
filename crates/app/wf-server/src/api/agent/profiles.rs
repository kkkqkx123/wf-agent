//! Agent profile surface: CRUD. Handlers are thin transport adapters over
//! the `wf-api::agent` profile surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use wf_storage::adapter::agent_profile::AgentProfileListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── agent profiles ──
        .route(
            "/agents",
            get(handle_list_profiles).post(handle_save_profile),
        )
        .route("/agents/validate", post(handle_validate_agent))
        .route(
            "/agents/{id}",
            get(handle_get_profile)
                .put(handle_update_profile)
                .delete(handle_delete_profile),
        )
}

// ── agent profiles ────────────────────────────────────────────────

/// Validate an agent definition through the wf-config processor without
/// persisting it.
async fn handle_validate_agent(
    State(_state): State<ApiState>,
    Json(definition): Json<wf_types::agent::AgentDefinition>,
) -> impl IntoResponse {
    match wf_api::infra::config::validate_agent(&definition) {
        Ok(()) => ok(true).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ListProfilesQuery {
    #[serde(flatten)]
    page: ListQuery,
    name: Option<String>,
    is_default: Option<bool>,
}

async fn handle_list_profiles(
    State(state): State<ApiState>,
    Query(query): Query<ListProfilesQuery>,
) -> impl IntoResponse {
    let options = AgentProfileListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        name_filter: query.name,
        is_default: query.is_default,
    };
    match wf_api::agent::agent::list_agent_profiles(&state.ctx.storage, Some(options)).await {
        Ok(profiles) => ok(profiles).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_profile(
    State(state): State<ApiState>,
    Json(profile): Json<wf_types::AgentProfileStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::agent::agent::save_agent_profile(&state.ctx.storage, &profile).await {
        Ok(()) => ok(profile.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::get_agent_profile(&state.ctx.storage, &path.id).await {
        Ok(profile) => ok(profile).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_update_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut profile): Json<wf_types::AgentProfileStorageMetadata>,
) -> impl IntoResponse {
    profile.id = wf_types::Id::from(path.id.clone());
    match wf_api::agent::agent::save_agent_profile(&state.ctx.storage, &profile).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::delete_agent_profile(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

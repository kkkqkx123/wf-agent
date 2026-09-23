//! Agent profile surface: CRUD. Handlers are thin transport adapters over
//! the `wf-api::agent` profile surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use wf_api::AgentProfileListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page};
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
#[utoipa::path(
    post,
    path = "/agents/validate",
    tag = "agent",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent definition is valid", body = bool),
        (status = 400, description = "Invalid agent definition", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_validate_agent(
    State(_state): State<ApiState>,
    Json(definition): Json<wf_api::AgentDefinition>,
) -> impl IntoResponse {
    match wf_api::infra::config::validate_agent(&definition) {
        Ok(()) => ok(true).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct ListProfilesQuery {
    #[serde(flatten)]
    page: ListQuery,
    /// Filter by profile name
    name: Option<String>,
    /// Filter by default status
    is_default: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/agents",
    tag = "agent",
    params(("limit" = Option<u64>, Query, description = "Page limit"), ("offset" = Option<u64>, Query, description = "Page offset")),
    responses(
        (status = 200, description = "List of agent profiles", body = serde_json::Value),
        (status = 400, description = "Invalid query parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_list_profiles(
    State(state): State<ApiState>,
    Query(query): Query<ListProfilesQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    let options = AgentProfileListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        name_filter: query.name,
        is_default: query.is_default,
    };
    match wf_api::agent::agent::list_agent_profiles(&state.ctx.storage, Some(options)).await {
        Ok(profiles) => ok_page(profiles, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/agents",
    tag = "agent",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent profile created", body = String),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Agent profile already exists", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_save_profile(
    State(state): State<ApiState>,
    Json(profile): Json<wf_api::AgentProfileStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::agent::agent::save_agent_profile(&state.ctx.storage, &profile).await {
        Ok(()) => ok(profile.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agents/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Agent profile ID")),
    responses(
        (status = 200, description = "Agent profile found", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_get_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::get_agent_profile(&state.ctx.storage, &path.id).await {
        Ok(profile) => ok(profile).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/agents/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Agent profile ID")),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent profile updated", body = String),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_update_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut profile): Json<wf_api::AgentProfileStorageMetadata>,
) -> impl IntoResponse {
    profile.id = wf_api::Id::from(path.id.clone());
    match wf_api::agent::agent::save_agent_profile(&state.ctx.storage, &profile).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/agents/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Agent profile ID")),
    responses(
        (status = 200, description = "Agent profile deleted", body = bool),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_delete_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent::delete_agent_profile(&state.ctx.storage, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
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

    #[tokio::test]
    async fn profiles_list_uses_cursor_envelope() {
        let ctx = make_ctx();
        let response = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/agents")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert!(body["data"]["items"].is_array());
        assert_eq!(body["data"]["limit"], 50);
        assert_eq!(body["data"]["offset"], 0);
        assert!(body["data"]["has_more"].is_boolean());
    }
}

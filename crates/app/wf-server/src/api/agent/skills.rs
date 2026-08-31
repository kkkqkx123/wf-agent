//! Skill entity surface: list / query / scan / reload, enable / disable,
//! cache management, content and resources. Handlers are thin transport
//! adapters over the `wf-api::entity::skill` surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::Value;

use wf_types::skill::SkillResourceType;

use crate::envelope::{error_response, ok};
use crate::extract::NamePath;
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── skills ──
        .route("/skills", get(handle_list_skills))
        .route("/skills/query", get(handle_query_skills))
        .route("/skills/scan", post(handle_scan_skills))
        .route("/skills/reload", post(handle_reload_skills))
        .route("/skills/enabled", get(handle_enabled_skills))
        .route("/skills/disabled", get(handle_disabled_skills))
        .route("/skills/cache/clear", post(handle_clear_skill_cache))
        .route(
            "/skills/cache/clear/{name}",
            post(handle_clear_skill_cache_by_name),
        )
        .route("/skills/{name}", get(handle_get_skill))
        .route("/skills/{name}/enable", post(handle_enable_skill))
        .route("/skills/{name}/disable", post(handle_disable_skill))
        .route("/skills/{name}/content", get(handle_skill_content))
        .route("/skills/{name}/resources", get(handle_skill_resources))
        .route("/skills/prompt", get(handle_skill_prompt))
}

// ── skills ────────────────────────────────────────────────────────

async fn handle_list_skills(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::skill::list_skills(&state.ctx) {
        Ok(skills) => ok(skills).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct QuerySkillsQuery {
    name: Option<String>,
    version: Option<String>,
    tags: Option<String>,
}

async fn handle_query_skills(
    State(state): State<ApiState>,
    Query(query): Query<QuerySkillsQuery>,
) -> impl IntoResponse {
    let filter = wf_api::SkillFilter {
        name: query.name,
        version: query.version,
        tags: query
            .tags
            .as_deref()
            .map(|t| t.split(',').map(ToOwned::to_owned).collect()),
    };
    match wf_api::entity::skill::query(&state.ctx, &filter) {
        Ok(skills) => ok(skills).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_skill(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
) -> impl IntoResponse {
    match wf_api::entity::skill::get_skill(&state.ctx, &path.name) {
        Ok(skill) => ok(skill).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_enable_skill(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
) -> impl IntoResponse {
    match wf_api::entity::skill::enable(&state.ctx, &path.name) {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_enabled_skills(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::skill::get_enabled_skills(&state.ctx) {
        Ok(skills) => ok(skills).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_disabled_skills(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::skill::get_disabled_skills(&state.ctx) {
        Ok(skills) => ok(skills).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_clear_skill_cache(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::skill::clear_cache(&state.ctx) {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_clear_skill_cache_by_name(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
) -> impl IntoResponse {
    match wf_api::entity::skill::clear_cache_by_name(&state.ctx, &path.name) {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_skill_content(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
) -> impl IntoResponse {
    match wf_api::entity::skill::load_content(&state.ctx, &path.name) {
        Ok(content) => ok(content).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_disable_skill(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
) -> impl IntoResponse {
    match wf_api::entity::skill::disable(&state.ctx, &path.name) {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SkillDirQuery {
    dir: Option<String>,
}

async fn handle_scan_skills(
    State(state): State<ApiState>,
    Query(query): Query<SkillDirQuery>,
) -> impl IntoResponse {
    match wf_api::entity::skill::scan_skills(&state.ctx, query.dir.as_deref().unwrap_or("")) {
        Ok(skills) => ok(skills).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_reload_skills(
    State(state): State<ApiState>,
    Query(query): Query<SkillDirQuery>,
) -> impl IntoResponse {
    match wf_api::entity::skill::reload(&state.ctx, query.dir.as_deref().unwrap_or("")) {
        Ok(skills) => ok(skills).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SkillResourcesQuery {
    resource_type: Option<String>,
}

async fn handle_skill_resources(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
    Query(query): Query<SkillResourcesQuery>,
) -> impl IntoResponse {
    let resource_type = match query.resource_type.as_deref() {
        Some("references") => SkillResourceType::References,
        Some("examples") => SkillResourceType::Examples,
        Some("scripts") => SkillResourceType::Scripts,
        Some("assets") => SkillResourceType::Assets,
        Some(other) => {
            return crate::envelope::err::<Value>(crate::envelope::ApiError::validation(format!(
                "unknown skill resource type: {other}"
            )))
            .into_response()
        }
        None => SkillResourceType::References,
    };
    match wf_api::entity::skill::load_resources(&state.ctx, &path.name, resource_type) {
        Ok(resources) => ok(resources).into_response(),
        Err(e) => error_response(e),
    }
}

/// Assemble the enabled-skill prompt: metadata block followed by the body
/// content of every enabled skill.
async fn handle_skill_prompt(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::skill::to_prompt(&state.ctx) {
        Ok(prompt) => ok(prompt).into_response(),
        Err(e) => error_response(e),
    }
}

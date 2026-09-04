//! Template library surface: library query / featured / popular / usage /
//! clone and the workflow / agent template registries. Split from
//! `api_templates` to keep the template surface at a maintainable file
//! size.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/templates/library", get(handle_query_library))
        .route("/templates/library/featured", get(handle_library_featured))
        .route("/templates/library/popular", get(handle_library_popular))
        .route("/templates/library/{id}/usage", post(handle_record_usage))
        .route("/templates/library/{id}/clone", post(handle_clone_template))
        // ── library registry (workflow / agent templates) ──
        .route(
            "/templates/library/workflows",
            get(handle_list_workflow_templates).post(handle_register_workflow_template),
        )
        .route(
            "/templates/library/workflows/{id}",
            get(handle_get_workflow_template).delete(handle_delete_workflow_template),
        )
        .route(
            "/templates/library/agents",
            get(handle_list_agent_templates).post(handle_register_agent_template),
        )
        .route(
            "/templates/library/agents/{id}",
            get(handle_get_agent_template).delete(handle_delete_agent_template),
        )
}

// ── template library ──────────────────────────────────────────────

#[derive(Deserialize)]
struct LibraryQuery {
    kind: Option<String>,
    name: Option<String>,
    category: Option<String>,
    author: Option<String>,
    tags: Option<String>,
}

async fn handle_query_library(
    State(state): State<ApiState>,
    Query(query): Query<LibraryQuery>,
) -> impl IntoResponse {
    let kind = match query.kind.as_deref() {
        Some("workflow") => Some(wf_api::TemplateKind::Workflow),
        Some("agent") => Some(wf_api::TemplateKind::Agent),
        Some(other) => {
            return crate::envelope::err::<serde_json::Value>(
                crate::envelope::ApiError::validation(format!("unknown template kind: {other}")),
            )
            .into_response()
        }
        None => None,
    };
    let filter = wf_api::TemplateFilter {
        kind,
        name: query.name,
        category: query.category,
        tags: query
            .tags
            .as_deref()
            .map(|t| t.split(',').map(ToOwned::to_owned).collect()),
        author: query.author,
    };
    match wf_api::template::template_library::query(&state.ctx, &filter) {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

async fn handle_library_featured(
    State(state): State<ApiState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    match wf_api::template::template_library::featured(&state.ctx, query.limit) {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct CategoryLimitQuery {
    category: Option<String>,
    limit: Option<usize>,
}

async fn handle_library_popular(
    State(state): State<ApiState>,
    Query(query): Query<CategoryLimitQuery>,
) -> impl IntoResponse {
    let result = match query.category {
        Some(category) => wf_api::template::template_library::popular_in_category(
            &state.ctx,
            &category,
            query.limit,
        ),
        None => wf_api::template::template_library::featured(&state.ctx, query.limit),
    };
    match result {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_record_usage(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    wf_api::template::template_library::record_usage(&state.ctx, &path.id);
    ok(wf_api::template::template_library::usage_count(
        &state.ctx, &path.id,
    ))
    .into_response()
}

#[derive(Deserialize)]
struct CloneTemplateBody {
    kind: String,
    new_name: Option<String>,
}

async fn handle_clone_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<CloneTemplateBody>,
) -> impl IntoResponse {
    let new_name = body.new_name.unwrap_or_default();
    let result: Result<serde_json::Value, wf_api::ApiError> = if body.kind == "agent" {
        wf_api::template::template_library::clone_agent_template(&state.ctx, &path.id, &new_name)
            .await
            .map(|t| serde_json::to_value(&t).unwrap_or_default())
    } else {
        wf_api::template::template_library::clone_workflow_template(&state.ctx, &path.id, &new_name)
            .map(|t| serde_json::to_value(&t).unwrap_or_default())
    };
    match result {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

// ── library registry (workflow / agent templates) ────────────────

async fn handle_list_workflow_templates(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::template::template_library::list_workflow_templates(&state.ctx) {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::get_workflow_template(&state.ctx, &path.id) {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_register_workflow_template(
    State(state): State<ApiState>,
    Json(template): Json<wf_types::workflow::WorkflowTemplate>,
) -> impl IntoResponse {
    match wf_api::template::template_library::register_workflow_template(&state.ctx, &template) {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::delete_workflow_template(&state.ctx, &path.id) {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_list_agent_templates(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::template::template_library::list_agent_templates(&state.ctx) {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::get_agent_template(&state.ctx, &path.id) {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_register_agent_template(
    State(state): State<ApiState>,
    Json(template): Json<wf_types::agent::AgentTemplate>,
) -> impl IntoResponse {
    match wf_api::template::template_library::register_agent_template(&state.ctx, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::delete_agent_template(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

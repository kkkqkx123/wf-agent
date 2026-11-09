//! Template library surface: unified library query / featured / popular /
//! usage and unified clone. Definition writes live on the definition
//! routes and fragment writes on the fragment routes; this module owns no
//! registry writes. Split from `api_templates` to keep the template
//! surface at a maintainable file size.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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
}

// ── template library ──────────────────────────────────────────────

/// Uniform template summary over workflow and agent templates.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct TemplateSummaryDoc {
    id: String,
    kind: String,
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    is_public: bool,
    enabled: bool,
    is_system: bool,
    is_published: bool,
    usage_count: u64,
    created_at: i64,
    updated_at: i64,
}

impl From<wf_api::template::template_library::TemplateSummary> for TemplateSummaryDoc {
    fn from(view: wf_api::template::template_library::TemplateSummary) -> Self {
        Self {
            id: view.id,
            kind: view.kind.to_string(),
            name: view.name,
            description: view.description,
            category: view.category,
            tags: view.tags,
            author: view.author,
            is_public: view.is_public,
            enabled: view.enabled,
            is_system: view.is_system,
            is_published: view.is_published,
            usage_count: view.usage_count,
            created_at: view.created_at,
            updated_at: view.updated_at,
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct LibraryQuery {
    kind: Option<String>,
    name: Option<String>,
    category: Option<String>,
    author: Option<String>,
    tags: Option<String>,
    include_system: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library",
    tag = "template",
    params(LibraryQuery),
    responses((status = 200, description = "Template library entries", body = crate::envelope::ApiEnvelope<Vec<TemplateSummaryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_query_library(
    State(state): State<ApiState>,
    Query(query): Query<LibraryQuery>,
) -> impl IntoResponse {
    let kind = match query.kind.as_deref() {
        Some("workflow") => Some(wf_api::TemplateKind::Workflow),
        Some("agent") => Some(wf_api::TemplateKind::Agent),
        Some("node") => Some(wf_api::TemplateKind::Node),
        Some("trigger") => Some(wf_api::TemplateKind::Trigger),
        Some("all") | None => None,
        Some(other) => {
            return crate::envelope::err(crate::envelope::InvalidParams::validation(format!(
                "unknown template kind: {other}"
            )))
            .into_response()
        }
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
        include_system: query.include_system,
    };
    match wf_api::template::template_library::query_unified(&state.ctx, &filter).await {
        Ok(templates) => ok(templates
            .into_iter()
            .map(TemplateSummaryDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/featured",
    tag = "template",
    params(LimitQuery),
    responses((status = 200, description = "Featured templates", body = crate::envelope::ApiEnvelope<Vec<TemplateSummaryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_library_featured(
    State(state): State<ApiState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    match wf_api::template::template_library::featured(&state.ctx, query.limit).await {
        Ok(templates) => ok(templates
            .into_iter()
            .map(TemplateSummaryDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct CategoryLimitQuery {
    category: Option<String>,
    limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/popular",
    tag = "template",
    params(CategoryLimitQuery),
    responses((status = 200, description = "Popular templates", body = crate::envelope::ApiEnvelope<Vec<TemplateSummaryDoc>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_library_popular(
    State(state): State<ApiState>,
    Query(query): Query<CategoryLimitQuery>,
) -> impl IntoResponse {
    let result = match query.category {
        Some(category) => {
            wf_api::template::template_library::popular_in_category(
                &state.ctx,
                &category,
                query.limit,
            )
            .await
        }
        None => wf_api::template::template_library::featured(&state.ctx, query.limit).await,
    };
    match result {
        Ok(templates) => ok(templates
            .into_iter()
            .map(TemplateSummaryDoc::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/{id}/usage",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_record_usage(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::record_usage(&state.ctx, &path.id).await {
        Ok(count) => ok(count).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct CloneTemplateBody {
    kind: String,
    new_name: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/{id}/clone",
    tag = "template",
    params(IdPath),
    request_body = CloneTemplateBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_clone_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<CloneTemplateBody>,
) -> impl IntoResponse {
    let new_name = body.new_name.unwrap_or_default();
    let result: Result<serde_json::Value, wf_api::ApiError> = if body.kind == "agent" {
        wf_api::template::template_library::clone_agent_template(&state.ctx, &path.id, &new_name)
            .await
            .map(|t| serde_json::to_value(&t).unwrap_or_default())
    } else if body.kind == "node" {
        wf_api::template::node_template::clone_template(&state.ctx, &path.id, &new_name)
            .await
            .map(|t| serde_json::to_value(&t).unwrap_or_default())
    } else if body.kind == "trigger" {
        wf_api::trigger::template::clone_template(&state.ctx, &path.id, &new_name)
            .await
            .map(|t| serde_json::to_value(&t).unwrap_or_default())
    } else if body.kind == "workflow" {
        let new_id = if new_name.is_empty() {
            None
        } else {
            Some(new_name.as_str())
        };
        wf_api::workflow::clone_workflow(&state.ctx, &path.id, new_id)
            .await
            .map(|id| serde_json::json!({"id": id}))
            .map_err(Into::into)
    } else {
        Err(wf_api::ApiError::Validation(format!(
            "unknown template kind for clone: {}",
            body.kind
        )))
    };
    match result {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

// ── library registry (workflow / agent templates) ────────────────

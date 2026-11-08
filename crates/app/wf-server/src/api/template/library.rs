//! Template library surface: library query / featured / popular / usage /
//! clone and the workflow / agent template registries. Split from
//! `api_templates` to keep the template surface at a maintainable file
//! size.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, IdVersionPath};
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
            "/templates/library/workflows/count",
            get(handle_count_workflow_templates),
        )
        .route(
            "/templates/library/workflows/import",
            post(handle_import_workflow_template),
        )
        .route(
            "/templates/library/workflows/{id}/export",
            get(handle_export_workflow_template),
        )
        .route(
            "/templates/library/workflows/{id}",
            get(handle_get_workflow_template)
                .put(handle_update_workflow_template)
                .delete(handle_delete_workflow_template),
        )
        .route(
            "/templates/library/workflows/{id}/publish",
            post(handle_publish_workflow_template),
        )
        .route(
            "/templates/library/workflows/{id}/unpublish",
            post(handle_unpublish_workflow_template),
        )
        .route(
            "/templates/library/workflows/{id}/clone",
            post(handle_clone_workflow_template),
        )
        .route(
            "/templates/library/workflows/{id}/validate",
            post(handle_validate_workflow_template),
        )
        .route(
            "/templates/library/workflows/{id}/versions",
            get(handle_list_workflow_template_versions).post(handle_save_workflow_template_version),
        )
        .route(
            "/templates/library/workflows/{id}/versions/{version}",
            get(handle_get_workflow_template_version),
        )
        .route(
            "/templates/library/workflows/{id}/rollback",
            post(handle_rollback_workflow_template),
        )
        .route(
            "/templates/library/agents",
            get(handle_list_agent_templates).post(handle_register_agent_template),
        )
        .route(
            "/templates/library/agents/count",
            get(handle_count_agent_templates),
        )
        .route(
            "/templates/library/agents/import",
            post(handle_import_agent_template),
        )
        .route(
            "/templates/library/agents/{id}/export",
            get(handle_export_agent_template),
        )
        .route(
            "/templates/library/agents/{id}",
            get(handle_get_agent_template)
                .put(handle_update_agent_template)
                .delete(handle_delete_agent_template),
        )
        .route(
            "/templates/library/agents/{id}/versions",
            get(handle_list_agent_template_versions).post(handle_save_agent_template_version),
        )
        .route(
            "/templates/library/agents/{id}/versions/{version}",
            get(handle_get_agent_template_version).delete(handle_delete_agent_template_version),
        )
        .route(
            "/templates/library/agents/{id}/rollback",
            post(handle_rollback_agent_template),
        )
        .route(
            "/templates/library/agents/{id}/publish",
            post(handle_publish_agent_template),
        )
        .route(
            "/templates/library/agents/{id}/unpublish",
            post(handle_unpublish_agent_template),
        )
        .route(
            "/templates/library/agents/{id}/validate",
            post(handle_validate_agent_template),
        )
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
    } else {
        Err(wf_api::ApiError::Validation(
            "workflow clone uses POST /api/v1/workflows/{id}/clone".to_string(),
        ))
    };
    match result {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

// ── library registry (workflow / agent templates) ────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct RegistryListQuery {
    /// Page limit; absent returns the full filtered list.
    limit: Option<u64>,
    /// Page offset; only meaningful with `limit`.
    offset: Option<u64>,
    /// Case-insensitive substring match on the template name.
    name: Option<String>,
    category: Option<String>,
    author: Option<String>,
    /// Comma-separated tags; a template matches when it carries any of them.
    tags: Option<String>,
}

impl RegistryListQuery {
    fn tag_list(&self) -> Vec<String> {
        self.tags
            .as_deref()
            .map(|raw| {
                raw.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Shared matcher over one entry's display attributes. Callers resolve
    /// their own attribute sources (template fields with metadata
    /// fallbacks) before matching.
    fn matches(&self, name: &str, category: Option<&str>, tags: Option<&[String]>, author: Option<&str>) -> bool {
        if let Some(filter_name) = self.name.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            if !name.to_lowercase().contains(&filter_name.to_lowercase()) {
                return false;
            }
        }
        if let Some(category_filter) = &self.category {
            if category != Some(category_filter.as_str()) {
                return false;
            }
        }
        let wanted = self.tag_list();
        if !wanted.is_empty() {
            let has_any = tags
                .map(|existing| wanted.iter().any(|tag| existing.contains(tag)))
                .unwrap_or(false);
            if !has_any {
                return false;
            }
        }
        if let Some(author_filter) = &self.author {
            if author != Some(author_filter.as_str()) {
                return false;
            }
        }
        true
    }
}

fn page_registry<T: Clone>(items: Vec<T>, limit: Option<u64>, offset: Option<u64>) -> Vec<T> {
    match (limit, offset) {
        (None, _) => items,
        (Some(limit), offset) => {
            let start = offset.unwrap_or(0) as usize;
            items.into_iter().skip(start).take(limit as usize).collect()
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/workflows",
    tag = "template",
    params(RegistryListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_workflow_templates(
    State(state): State<ApiState>,
    Query(query): Query<RegistryListQuery>,
) -> impl IntoResponse {
    match wf_api::template::template_library::list_workflow_templates(&state.ctx) {
        Ok(templates) => {
            let filtered = templates
                .into_iter()
                .filter(|t| {
                    let category = t.template_category.as_deref().or_else(|| {
                        t.definition.metadata.as_ref().and_then(|m| m.category.as_deref())
                    });
                    let tags = t.template_tags.as_ref().or_else(|| {
                        t.definition.metadata.as_ref().and_then(|m| m.tags.as_ref())
                    });
                    let author = t
                        .definition
                        .metadata
                        .as_ref()
                        .and_then(|m| m.author.as_deref());
                    query.matches(&t.name, category, tags.map(|v| v.as_slice()), author)
                })
                .collect();
            ok(page_registry(filtered, query.limit, query.offset)).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/workflows/count",
    tag = "template",
    params(RegistryListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::template::templates::TemplateCountView>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_count_workflow_templates(
    State(state): State<ApiState>,
    Query(query): Query<RegistryListQuery>,
) -> impl IntoResponse {
    match wf_api::template::template_library::list_workflow_templates(&state.ctx) {
        Ok(templates) => {
            let count = templates
                .iter()
                .filter(|t| {
                    let category = t.template_category.as_deref().or_else(|| {
                        t.definition.metadata.as_ref().and_then(|m| m.category.as_deref())
                    });
                    let tags = t.template_tags.as_ref().or_else(|| {
                        t.definition.metadata.as_ref().and_then(|m| m.tags.as_ref())
                    });
                    let author = t
                        .definition
                        .metadata
                        .as_ref()
                        .and_then(|m| m.author.as_deref());
                    query.matches(&t.name, category, tags.map(|v| v.as_slice()), author)
                })
                .count();
            ok(crate::api::template::templates::TemplateCountView { count }).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/workflows/{id}",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::get_workflow_template(&state.ctx, &path.id) {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/workflows",
    tag = "template",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_register_workflow_template(
    State(state): State<ApiState>,
    Json(template): Json<wf_api::WorkflowTemplate>,
) -> impl IntoResponse {
    match wf_api::template::template_library::register_workflow_template(&state.ctx, &template)
        .await
    {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/templates/library/workflows/{id}",
    tag = "template",
    params(IdPath),
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut template): Json<wf_api::WorkflowTemplate>,
) -> impl IntoResponse {
    template.id = wf_api::Id::from(path.id.clone());
    match wf_api::template::template_library::update_workflow_template(&state.ctx, &template).await
    {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/templates/library/workflows/{id}",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::delete_workflow_template(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/workflows/{id}/publish",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_publish_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::publish_workflow_template(&state.ctx, &path.id).await
    {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/workflows/{id}/unpublish",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unpublish_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::unpublish_workflow_template(&state.ctx, &path.id)
        .await
    {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/agents",
    tag = "template",
    params(RegistryListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_agent_templates(
    State(state): State<ApiState>,
    Query(query): Query<RegistryListQuery>,
) -> impl IntoResponse {
    match wf_api::template::template_library::list_agent_templates(&state.ctx) {
        Ok(templates) => {
            let filtered = templates
                .into_iter()
                .filter(|t| {
                    let category = t.template_category.as_deref().or_else(|| {
                        t.definition.metadata.as_ref().and_then(|m| m.category.as_deref())
                    });
                    let tags = t.template_tags.as_ref().or_else(|| {
                        t.definition.metadata.as_ref().and_then(|m| m.tags.as_ref())
                    });
                    let author = t
                        .definition
                        .metadata
                        .as_ref()
                        .and_then(|m| m.author.as_deref());
                    query.matches(&t.name, category, tags.map(|v| v.as_slice()), author)
                })
                .collect();
            ok(page_registry(filtered, query.limit, query.offset)).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/agents/count",
    tag = "template",
    params(RegistryListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::template::templates::TemplateCountView>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_count_agent_templates(
    State(state): State<ApiState>,
    Query(query): Query<RegistryListQuery>,
) -> impl IntoResponse {
    match wf_api::template::template_library::list_agent_templates(&state.ctx) {
        Ok(templates) => {
            let count = templates
                .iter()
                .filter(|t| {
                    let category = t.template_category.as_deref().or_else(|| {
                        t.definition.metadata.as_ref().and_then(|m| m.category.as_deref())
                    });
                    let tags = t.template_tags.as_ref().or_else(|| {
                        t.definition.metadata.as_ref().and_then(|m| m.tags.as_ref())
                    });
                    let author = t
                        .definition
                        .metadata
                        .as_ref()
                        .and_then(|m| m.author.as_deref());
                    query.matches(&t.name, category, tags.map(|v| v.as_slice()), author)
                })
                .count();
            ok(crate::api::template::templates::TemplateCountView { count }).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/agents/{id}",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::get_agent_template(&state.ctx, &path.id) {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/agents",
    tag = "template",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_register_agent_template(
    State(state): State<ApiState>,
    Json(template): Json<wf_api::AgentTemplate>,
) -> impl IntoResponse {
    match wf_api::template::template_library::register_agent_template(&state.ctx, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/templates/library/agents/{id}",
    tag = "template",
    params(IdPath),
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut template): Json<wf_api::AgentTemplate>,
) -> impl IntoResponse {
    template.id = wf_api::Id::from(path.id.clone());
    match wf_api::template::template_library::update_agent_template(&state.ctx, &template).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/templates/library/agents/{id}",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::delete_agent_template(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

// ── library import / export ───────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub(crate) struct LibraryImportBody {
    json: String,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct LibraryExportQuery {
    download: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/workflows/import",
    tag = "template",
    request_body = LibraryImportBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_workflow_template(
    State(state): State<ApiState>,
    Json(body): Json<LibraryImportBody>,
) -> impl IntoResponse {
    let template: wf_api::WorkflowTemplate =
        match serde_json::from_str(&body.json).map_err(wf_api::ApiError::from) {
            Ok(template) => template,
            Err(e) => return error_response(e),
        };
    match wf_api::template::template_library::register_workflow_template(&state.ctx, &template)
        .await
    {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/agents/import",
    tag = "template",
    request_body = LibraryImportBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_agent_template(
    State(state): State<ApiState>,
    Json(body): Json<LibraryImportBody>,
) -> impl IntoResponse {
    let template: wf_api::AgentTemplate =
        match serde_json::from_str(&body.json).map_err(wf_api::ApiError::from) {
            Ok(template) => template,
            Err(e) => return error_response(e),
        };
    match wf_api::template::template_library::register_agent_template(&state.ctx, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/workflows/{id}/export",
    tag = "template",
    params(IdPath, LibraryExportQuery),
    responses((status = 200, description = "Exported workflow template file download", body = String, content_type = "application/json"), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_export_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<LibraryExportQuery>,
) -> impl IntoResponse {
    match wf_api::template::template_library::get_workflow_template(&state.ctx, &path.id) {
        Ok(template) => {
            if query.download.unwrap_or(false) {
                match serde_json::to_string_pretty(&template) {
                    Ok(json) => crate::envelope::download(
                        &json,
                        "application/json",
                        &format!("workflow-template-{}.json", path.id),
                    )
                    .into_response(),
                    Err(e) => error_response(wf_api::ApiError::from(e)),
                }
            } else {
                ok(template).into_response()
            }
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/agents/{id}/export",
    tag = "template",
    params(IdPath, LibraryExportQuery),
    responses((status = 200, description = "Exported agent template file download", body = String, content_type = "application/json"), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_export_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<LibraryExportQuery>,
) -> impl IntoResponse {
    match wf_api::template::template_library::get_agent_template(&state.ctx, &path.id) {
        Ok(template) => {
            if query.download.unwrap_or(false) {
                match serde_json::to_string_pretty(&template) {
                    Ok(json) => crate::envelope::download(
                        &json,
                        "application/json",
                        &format!("agent-template-{}.json", path.id),
                    )
                    .into_response(),
                    Err(e) => error_response(wf_api::ApiError::from(e)),
                }
            } else {
                ok(template).into_response()
            }
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/agents/{id}/versions",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Agent template version list", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_agent_template_versions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::list_agent_template_versions(&state.ctx, &path.id)
        .await
    {
        Ok(versions) => ok(versions).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/agents/{id}/versions/{version}",
    tag = "template",
    params(IdVersionPath),
    responses((status = 200, description = "Agent template version detail", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_agent_template_version(
    State(state): State<ApiState>,
    Path(path): Path<IdVersionPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::get_agent_template_version(
        &state.ctx,
        &path.id,
        &path.version,
    )
    .await
    {
        Ok(version) => ok(version).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct SaveAgentTemplateVersionBody {
    version: String,
    #[schema(value_type = Object)]
    template: wf_api::AgentTemplate,
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/agents/{id}/versions",
    tag = "template",
    params(IdPath),
    request_body = SaveAgentTemplateVersionBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_agent_template_version(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<SaveAgentTemplateVersionBody>,
) -> impl IntoResponse {
    match wf_api::template::template_library::save_agent_template_version(
        &state.ctx,
        &path.id,
        &body.version,
        &body.template,
    )
    .await
    {
        Ok(()) => ok(body.version).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct RollbackAgentTemplateBody {
    version: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/agents/{id}/rollback",
    tag = "template",
    params(IdPath),
    request_body = RollbackAgentTemplateBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rollback_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<RollbackAgentTemplateBody>,
) -> impl IntoResponse {
    match wf_api::template::template_library::rollback_agent_template(
        &state.ctx,
        &path.id,
        &body.version,
    )
    .await
    {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/templates/library/agents/{id}/versions/{version}",
    tag = "template",
    params(IdVersionPath),
    responses((status = 200, description = "Agent template version deleted", body = crate::envelope::ApiEnvelope<bool>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_agent_template_version(
    State(state): State<ApiState>,
    Path(path): Path<IdVersionPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::delete_agent_template_version(
        &state.ctx,
        &path.id,
        &path.version,
    )
    .await
    {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/agents/{id}/publish",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_publish_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::publish_agent_template(&state.ctx, &path.id).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/agents/{id}/unpublish",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unpublish_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::unpublish_agent_template(&state.ctx, &path.id).await
    {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/agents/{id}/validate",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Validation warnings (empty when valid)", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid template", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_validate_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    let template =
        match wf_api::template::template_library::get_agent_template(&state.ctx, &path.id) {
            Ok(template) => template,
            Err(e) => return error_response(e),
        };
    match wf_api::template::template_library::validate_agent_template_definition(
        &state.ctx,
        &template,
    )
    .await
    {
        Ok(warnings) => ok(warnings).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct CloneWorkflowTemplateBody {
    new_id: Option<String>,
}

/// Library-scoped clone of a workflow template. The template and the
/// workflow definition share one document, so this delegates to the
/// workflow clone; the alias exists so library clients need no second
/// endpoint family.
#[utoipa::path(
    post,
    path = "/api/v1/templates/library/workflows/{id}/clone",
    tag = "template",
    params(IdPath),
    request_body = CloneWorkflowTemplateBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_clone_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<CloneWorkflowTemplateBody>,
) -> impl IntoResponse {
    match wf_api::workflow::clone_workflow(&state.ctx, &path.id, body.new_id.as_deref()).await {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/workflows/{id}/validate",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<bool>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid template", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_validate_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    let template =
        match wf_api::template::template_library::get_workflow_template(&state.ctx, &path.id) {
            Ok(template) => template,
            Err(e) => return error_response(e),
        };
    match wf_api::workflow::validate_workflow(&template.definition) {
        Ok(()) => ok(true).into_response(),
        Err(e) => error_response(e),
    }
}

/// Library aliases over the workflow version store. A library workflow
/// template and its workflow definition are one document under one id,
/// so versions live in the workflow version surface; these routes expose
/// the same operations under the library prefix.
#[utoipa::path(
    get,
    path = "/api/v1/templates/library/workflows/{id}/versions",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Workflow template version list", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_workflow_template_versions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::list_workflow_versions(&state.ctx, &path.id).await {
        Ok(versions) => ok(versions).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/library/workflows/{id}/versions/{version}",
    tag = "template",
    params(IdVersionPath),
    responses((status = 200, description = "Workflow template version detail", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_workflow_template_version(
    State(state): State<ApiState>,
    Path(path): Path<IdVersionPath>,
) -> impl IntoResponse {
    match wf_api::workflow::get_workflow_version(&state.ctx, &path.id, &path.version).await {
        Ok(version) => ok(version).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct SaveWorkflowTemplateVersionBody {
    version: String,
    #[schema(value_type = Object)]
    workflow: wf_api::WorkflowDefinition,
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/workflows/{id}/versions",
    tag = "template",
    params(IdPath),
    request_body = SaveWorkflowTemplateVersionBody,
    responses((status = 200, description = "Workflow template version saved", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_workflow_template_version(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<SaveWorkflowTemplateVersionBody>,
) -> impl IntoResponse {
    match wf_api::workflow::save_workflow_version(
        &state.ctx,
        &path.id,
        &body.version,
        &body.workflow,
    )
    .await
    {
        Ok(()) => ok(body.version).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct RollbackWorkflowTemplateBody {
    version: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/library/workflows/{id}/rollback",
    tag = "template",
    params(IdPath),
    request_body = RollbackWorkflowTemplateBody,
    responses((status = 200, description = "Workflow template rolled back", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rollback_workflow_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<RollbackWorkflowTemplateBody>,
) -> impl IntoResponse {
    match wf_api::workflow::rollback_workflow(&state.ctx, &path.id, &body.version).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

//! Agent definition surface: CRUD, import/export, publish, validate,
//! clone, versions and rollback. Handlers are thin transport adapters over
//! the `wf_api::template::template_library` agent surface; the persisted
//! form is the agent definition, the registry holds the composed view.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, IdVersionPath};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/agent-definitions",
            get(handle_list_agent_templates).post(handle_register_agent_template),
        )
        .route(
            "/agent-definitions/import",
            post(handle_import_agent_template),
        )
        .route(
            "/agent-definitions/{id}/export",
            get(handle_export_agent_template),
        )
        .route(
            "/agent-definitions/{id}",
            get(handle_get_agent_template)
                .put(handle_update_agent_template)
                .delete(handle_delete_agent_template),
        )
        .route(
            "/agent-definitions/{id}/versions",
            get(handle_list_agent_template_versions).post(handle_save_agent_template_version),
        )
        .route(
            "/agent-definitions/{id}/versions/{version}",
            get(handle_get_agent_template_version).delete(handle_delete_agent_template_version),
        )
        .route(
            "/agent-definitions/{id}/rollback",
            post(handle_rollback_agent_template),
        )
        .route(
            "/agent-definitions/{id}/publish",
            post(handle_publish_agent_template),
        )
        .route(
            "/agent-definitions/{id}/unpublish",
            post(handle_unpublish_agent_template),
        )
        .route(
            "/agent-definitions/{id}/validate",
            post(handle_validate_agent_template),
        )
        .route(
            "/agent-definitions/{id}/clone",
            post(handle_clone_agent_definition),
        )
}

// ── agent definitions ─────────────────────────────────────────────
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
    /// Delegate to the shared template filter so fragment and library
    /// listings cannot drift apart. Attribute sources stay with the caller.
    fn to_basic_filter(&self) -> wf_api::template::BasicTemplateFilter {
        wf_api::template::BasicTemplateFilter::from_query(
            self.name.clone(),
            self.category.clone(),
            self.tags.as_deref(),
            self.author.clone(),
        )
    }

    fn matches(
        &self,
        name: &str,
        category: Option<&str>,
        tags: Option<&[String]>,
        author: Option<&str>,
    ) -> bool {
        self.to_basic_filter().matches(name, category, tags, author)
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

#[derive(Deserialize, ToSchema)]
pub(crate) struct LibraryImportBody {
    json: String,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct LibraryExportQuery {
    download: Option<bool>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct SaveAgentTemplateVersionBody {
    version: String,
    #[schema(value_type = Object)]
    definition: wf_api::AgentDefinition,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct RollbackAgentTemplateBody {
    version: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/agent-definitions",
    tag = "agent",
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
                        t.definition
                            .metadata
                            .as_ref()
                            .and_then(|m| m.category.as_deref())
                    });
                    let tags = t
                        .template_tags
                        .as_ref()
                        .or_else(|| t.definition.metadata.as_ref().and_then(|m| m.tags.as_ref()));
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
    path = "/api/v1/agent-definitions/{id}",
    tag = "agent",
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
    path = "/api/v1/agent-definitions",
    tag = "agent",
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
    path = "/api/v1/agent-definitions/{id}",
    tag = "agent",
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
    path = "/api/v1/agent-definitions/{id}",
    tag = "agent",
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

#[utoipa::path(
    post,
    path = "/api/v1/agent-definitions/import",
    tag = "agent",
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
    path = "/api/v1/agent-definitions/{id}/export",
    tag = "agent",
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
    path = "/api/v1/agent-definitions/{id}/versions",
    tag = "agent",
    params(IdPath),
    responses((status = 200, description = "Agent definition version list", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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
    path = "/api/v1/agent-definitions/{id}/versions/{version}",
    tag = "agent",
    params(IdVersionPath),
    responses((status = 200, description = "Agent definition version detail", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
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

#[utoipa::path(
    post,
    path = "/api/v1/agent-definitions/{id}/versions",
    tag = "agent",
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
        &body.definition,
    )
    .await
    {
        Ok(()) => ok(body.version).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-definitions/{id}/rollback",
    tag = "agent",
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
    path = "/api/v1/agent-definitions/{id}/versions/{version}",
    tag = "agent",
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
    path = "/api/v1/agent-definitions/{id}/publish",
    tag = "agent",
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
    path = "/api/v1/agent-definitions/{id}/unpublish",
    tag = "agent",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<String>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_unpublish_agent_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::template_library::unpublish_agent_template(&state.ctx, &path.id).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-definitions/{id}/validate",
    tag = "agent",
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
        &state.ctx, &template,
    )
    .await
    {
        Ok(warnings) => ok(warnings).into_response(),
        Err(e) => error_response(e),
    }
}
#[derive(Deserialize, ToSchema)]
pub(crate) struct CloneAgentDefinitionBody {
    new_name: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/agent-definitions/{id}/clone",
    tag = "agent",
    params(IdPath),
    request_body = CloneAgentDefinitionBody,
    responses((status = 200, description = "Cloned agent definition with its library view", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_clone_agent_definition(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<CloneAgentDefinitionBody>,
) -> impl IntoResponse {
    let new_name = body.new_name.unwrap_or_default();
    match wf_api::template::template_library::clone_agent_template(&state.ctx, &path.id, &new_name)
        .await
    {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

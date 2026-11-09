//! Template domain: node / trigger template CRUD with export-import. The
//! shared template library lives in `template/library`.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use wf_api::NodeTemplateListOptions;
use wf_api::{NodeTemplateStorageMetadata, TriggerTemplateStorageMetadata};

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::paged::{fetch_size, ok_page, resolve_page_fields};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── node templates ──
        .route(
            "/templates/node",
            get(handle_list_node_templates).post(handle_save_node_template),
        )
        .route("/templates/node/import", post(handle_import_node_template))
        .route(
            "/templates/node/batch-delete",
            post(handle_batch_delete_node_templates),
        )
        .route("/templates/node/count", get(handle_count_node_templates))
        .route(
            "/templates/node/{id}",
            get(handle_get_node_template)
                .put(handle_update_node_template)
                .delete(handle_delete_node_template),
        )
        .route(
            "/templates/node/{id}/export",
            get(handle_export_node_template),
        )
        // ── trigger templates (storage-backed agent trigger templates) ──
        .route(
            "/templates/trigger",
            get(handle_list_trigger_templates).post(handle_save_trigger_template),
        )
        .route(
            "/templates/trigger/import",
            post(handle_import_trigger_template),
        )
        .route(
            "/templates/trigger/search",
            get(handle_search_trigger_templates),
        )
        .route(
            "/templates/trigger/batch-delete",
            post(handle_batch_delete_trigger_templates),
        )
        .route(
            "/templates/trigger/count",
            get(handle_count_trigger_templates),
        )
        .route(
            "/templates/trigger/{id}",
            get(handle_get_trigger_template)
                .put(handle_update_trigger_template)
                .delete(handle_delete_trigger_template),
        )
        .route(
            "/templates/trigger/{id}/export",
            get(handle_export_trigger_template),
        )
}

// ── node templates ────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListNodeTemplatesQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    node_type: Option<String>,
    /// Case-insensitive substring match on the template name.
    q: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/node",
    tag = "template",
    params(ListNodeTemplatesQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_node_templates(
    State(state): State<ApiState>,
    Query(query): Query<ListNodeTemplatesQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    // A name query cannot be pushed to storage (the filter language only
    // covers equality), so the filtered path lists without storage
    // pagination and pages the matches in memory; the unfiltered path
    // keeps the storage-level window exactly as before.
    if let Some(keyword) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let options = NodeTemplateListOptions {
            offset: None,
            limit: None,
            node_type_filter: query.node_type,
        };
        let needle = keyword.to_lowercase();
        return match wf_api::template::node_template::node_template_summaries(
            &state.ctx.storage,
            Some(options),
        )
        .await
        {
            Ok(templates) => {
                let window = templates
                    .into_iter()
                    .filter(|t| t.name.to_lowercase().contains(&needle))
                    .skip(offset as usize)
                    .take(fetch_size(limit) as usize)
                    .collect();
                ok_page(window, limit, offset).into_response()
            }
            Err(e) => error_response(e),
        };
    }
    let options = NodeTemplateListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        node_type_filter: query.node_type,
    };
    match wf_api::template::node_template::node_template_summaries(
        &state.ctx.storage,
        Some(options),
    )
    .await
    {
        Ok(templates) => ok_page(templates, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/node",
    tag = "template",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_node_template(
    State(state): State<ApiState>,
    Json(template): Json<NodeTemplateStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::template::node_template::save_node_template(&state.ctx, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/node/{id}",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::node_template::get_node_template(&state.ctx.storage, &path.id).await {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/templates/node/{id}",
    tag = "template",
    params(IdPath),
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut template): Json<NodeTemplateStorageMetadata>,
) -> impl IntoResponse {
    template.id = wf_api::Id::from(path.id.clone());
    match wf_api::template::node_template::save_node_template(&state.ctx, &template).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/templates/node/{id}",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::node_template::delete_node_template(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/node/{id}/export",
    tag = "template",
    params(IdPath, ExportTemplateQuery),
    responses((status = 200, description = "Exported node template file download", body = String, content_type = "application/json"), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_export_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ExportTemplateQuery>,
) -> impl IntoResponse {
    match wf_api::template::node_template::export_template(&state.ctx.storage, &path.id).await {
        Ok(json) => {
            if query.download.unwrap_or(false) {
                crate::envelope::download(
                    &json,
                    "application/json",
                    &format!("node-template-{}.json", path.id),
                )
                .into_response()
            } else {
                ok(json).into_response()
            }
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ExportTemplateQuery {
    download: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct ImportBody {
    json: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/node/import",
    tag = "template",
    request_body = ImportBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_node_template(
    State(state): State<ApiState>,
    Json(body): Json<ImportBody>,
) -> impl IntoResponse {
    match wf_api::template::node_template::import_template(&state.ctx, &body.json).await {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

// ── trigger templates ─────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListTriggerTemplatesQuery {
    trigger_type: Option<String>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    name: Option<String>,
    category: Option<String>,
    /// Comma-separated tags; a template matches when it carries any of them.
    tags: Option<String>,
    enabled: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger",
    tag = "template",
    params(ListTriggerTemplatesQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_trigger_templates(
    State(state): State<ApiState>,
    Query(query): Query<ListTriggerTemplatesQuery>,
) -> impl IntoResponse {
    let filter = wf_api::AgentTriggerTemplateFilter {
        trigger_type: query.trigger_type,
        category: query.category,
        tags: query.tags.as_deref().map(|raw| {
            raw.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        }),
        enabled: query.enabled,
        name: query.name,
    };
    match wf_api::trigger::template::summaries(&state.ctx, Some(&filter)).await {
        Ok(templates) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
            let window = templates
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger",
    tag = "template",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_trigger_template(
    State(state): State<ApiState>,
    Json(template): Json<TriggerTemplateStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::trigger::template::save(&state.ctx, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/{id}",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_trigger_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::trigger::template::get(&state.ctx, &path.id).await {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/templates/trigger/{id}",
    tag = "template",
    params(IdPath),
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_trigger_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut template): Json<TriggerTemplateStorageMetadata>,
) -> impl IntoResponse {
    template.id = wf_api::Id::from(path.id.clone());
    match wf_api::trigger::template::save(&state.ctx, &template).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/templates/trigger/{id}",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_trigger_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::trigger::template::delete(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/{id}/export",
    tag = "template",
    params(IdPath, ExportTemplateQuery),
    responses((status = 200, description = "Exported trigger template file download", body = String, content_type = "application/json"), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_export_trigger_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ExportTemplateQuery>,
) -> impl IntoResponse {
    match wf_api::trigger::template::export_template(&state.ctx, &path.id).await {
        Ok(json) => {
            if query.download.unwrap_or(false) {
                crate::envelope::download(
                    &json,
                    "application/json",
                    &format!("trigger-template-{}.json", path.id),
                )
                .into_response()
            } else {
                ok(json).into_response()
            }
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger/import",
    tag = "template",
    request_body = ImportBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_trigger_template(
    State(state): State<ApiState>,
    Json(body): Json<ImportBody>,
) -> impl IntoResponse {
    match wf_api::trigger::template::import_template(&state.ctx, &body.json).await {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct SearchTriggerTemplatesQuery {
    /// Substring matched against template names and descriptions.
    q: Option<String>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/search",
    tag = "template",
    params(SearchTriggerTemplatesQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_search_trigger_templates(
    State(state): State<ApiState>,
    Query(query): Query<SearchTriggerTemplatesQuery>,
) -> impl IntoResponse {
    let keyword = query.q.as_deref().map(str::trim).unwrap_or_default();
    if keyword.is_empty() {
        return crate::envelope::err(crate::envelope::InvalidParams::validation(
            "q must not be empty".to_string(),
        ))
        .into_response();
    }
    match wf_api::trigger::template::search(&state.ctx, keyword).await {
        Ok(templates) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
            let window = templates
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct TemplateCountView {
    pub(crate) count: usize,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/node/count",
    tag = "template",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::template::templates::TemplateCountView>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_count_node_templates(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match wf_api::template::node_template::list_node_templates(&state.ctx.storage, None).await {
        Ok(templates) => ok(TemplateCountView {
            count: templates.len(),
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/count",
    tag = "template",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::template::templates::TemplateCountView>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_count_trigger_templates(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match wf_api::trigger::template::query(&state.ctx, None).await {
        Ok(templates) => ok(TemplateCountView {
            count: templates.len(),
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct TemplateIdsBody {
    ids: Vec<String>,
}

/// Maximum ids per batch; matches the execution batch surface.
const MAX_TEMPLATE_BATCH_IDS: usize = 100;

fn validate_template_ids(ids: &[String]) -> Result<(), wf_api::ApiError> {
    if ids.is_empty() {
        return Err(wf_api::ApiError::Validation(
            "ids must not be empty".to_string(),
        ));
    }
    if ids.len() > MAX_TEMPLATE_BATCH_IDS {
        return Err(wf_api::ApiError::Validation(format!(
            "at most {MAX_TEMPLATE_BATCH_IDS} ids per batch"
        )));
    }
    Ok(())
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/node/batch-delete",
    tag = "template",
    request_body = TemplateIdsBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_batch_delete_node_templates(
    State(state): State<ApiState>,
    Json(body): Json<TemplateIdsBody>,
) -> impl IntoResponse {
    if let Err(e) = validate_template_ids(&body.ids) {
        return error_response(e);
    }
    let mut results = Vec::with_capacity(body.ids.len());
    for id in &body.ids {
        match wf_api::template::node_template::delete_node_template(&state.ctx, id).await {
            Ok(_) => results.push(crate::api::web::batch::BatchItemResult {
                id: id.clone(),
                ok: true,
                error: None,
            }),
            Err(e) => results.push(crate::api::web::batch::BatchItemResult {
                id: id.clone(),
                ok: false,
                error: Some(e.to_string()),
            }),
        }
    }
    ok(results).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger/batch-delete",
    tag = "template",
    request_body = TemplateIdsBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_batch_delete_trigger_templates(
    State(state): State<ApiState>,
    Json(body): Json<TemplateIdsBody>,
) -> impl IntoResponse {
    if let Err(e) = validate_template_ids(&body.ids) {
        return error_response(e);
    }
    let mut results = Vec::with_capacity(body.ids.len());
    for id in &body.ids {
        match wf_api::trigger::template::delete(&state.ctx, id).await {
            Ok(_) => results.push(crate::api::web::batch::BatchItemResult {
                id: id.clone(),
                ok: true,
                error: None,
            }),
            Err(e) => results.push(crate::api::web::batch::BatchItemResult {
                id: id.clone(),
                ok: false,
                error: Some(e.to_string()),
            }),
        }
    }
    ok(results).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use axum::response::Response;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn template_endpoints_are_reachable() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/templates/node",
            "/api/v1/templates/trigger",
            "/api/v1/agent-definitions",
            "/api/v1/templates/library",
            "/api/v1/templates/library/featured",
            "/api/v1/templates/library/popular",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
        // Usage recording is a POST endpoint.
        let usage = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/templates/library/some-id/usage")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(usage.status(), StatusCode::OK);
        // Single-resource routes map unknown ids to NotFound.
        for uri in [
            "/api/v1/templates/node/missing",
            "/api/v1/templates/trigger/missing",
            "/api/v1/templates/node/drafts/missing",
            "/api/v1/templates/trigger/drafts/missing",
            "/api/v1/templates/node/missing/versions/v1",
            "/api/v1/templates/trigger/missing/versions/v1",
            "/api/v1/agent-definitions/missing",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
        // The definition registries are queryable even when empty.
        for uri in [
            "/api/v1/agent-definitions",
            "/api/v1/templates/node/drafts",
            "/api/v1/templates/trigger/drafts",
            "/api/v1/templates/node/missing/versions",
            "/api/v1/templates/trigger/missing/versions",
            "/api/v1/templates/node/missing/lock",
            "/api/v1/templates/trigger/missing/lock",
            "/api/v1/templates/node/missing/lifecycle",
            "/api/v1/templates/trigger/missing/lifecycle",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
    }
}

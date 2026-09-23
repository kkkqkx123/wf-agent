//! Template domain: node / trigger template CRUD with export-import. Agent
//! trigger / agent template query surfaces live in `template/queries` and
//! the shared template library in `template/library`.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use wf_api::NodeTemplateListOptions;
use wf_api::{NodeTemplateStorageMetadata, TriggerTemplateStorageMetadata};

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page};
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

#[derive(Deserialize)]
pub(crate) struct ListNodeTemplatesQuery {
    #[serde(flatten)]
    page: ListQuery,
    node_type: Option<String>,
}

#[utoipa::path(
    get,
    path = "/templates/node",
    tag = "template",
    params(("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset"), ("node_type" = Option<String>, Query, description = "node_type")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_node_templates(
    State(state): State<ApiState>,
    Query(query): Query<ListNodeTemplatesQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
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
    path = "/templates/node",
    tag = "template",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_node_template(
    State(state): State<ApiState>,
    Json(template): Json<NodeTemplateStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::template::node_template::save_node_template_indexed(&state.ctx, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/templates/node/{id}",
    tag = "template",
    params(("id" = String, Path, description = "id")),
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
    path = "/templates/node/{id}",
    tag = "template",
    params(("id" = String, Path, description = "id")),
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
    match wf_api::template::node_template::save_node_template_indexed(&state.ctx, &template).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/templates/node/{id}",
    tag = "template",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::node_template::delete_node_template_indexed(&state.ctx, &path.id).await
    {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/templates/node/{id}/export",
    tag = "template",
    params(("id" = String, Path, description = "id"), ("download" = Option<bool>, Query, description = "download")),
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

#[derive(Deserialize)]
pub(crate) struct ExportTemplateQuery {
    download: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct ImportBody {
    json: String,
}

#[utoipa::path(
    post,
    path = "/templates/node/import",
    tag = "template",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_node_template(
    State(state): State<ApiState>,
    Json(body): Json<ImportBody>,
) -> impl IntoResponse {
    match wf_api::template::node_template::import_template_indexed(&state.ctx, &body.json).await {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

// ── trigger templates ─────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct ListTriggerTemplatesQuery {
    trigger_type: Option<String>,
    #[serde(flatten)]
    page: ListQuery,
}

#[utoipa::path(
    get,
    path = "/templates/trigger",
    tag = "template",
    params(("trigger_type" = Option<String>, Query, description = "trigger_type"), ("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_trigger_templates(
    State(state): State<ApiState>,
    Query(query): Query<ListTriggerTemplatesQuery>,
) -> impl IntoResponse {
    let filter = wf_api::AgentTriggerTemplateFilter {
        trigger_type: query.trigger_type,
        category: None,
        tags: None,
        enabled: None,
        name: None,
    };
    match wf_api::trigger::template::summaries(&state.ctx, Some(&filter)).await {
        Ok(templates) => {
            let (limit, offset) = resolve_page(&query.page);
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
    path = "/templates/trigger",
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
    path = "/templates/trigger/{id}",
    tag = "template",
    params(("id" = String, Path, description = "id")),
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
    path = "/templates/trigger/{id}",
    tag = "template",
    params(("id" = String, Path, description = "id")),
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
    path = "/templates/trigger/{id}",
    tag = "template",
    params(("id" = String, Path, description = "id")),
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
    path = "/templates/trigger/{id}/export",
    tag = "template",
    params(("id" = String, Path, description = "id"), ("download" = Option<bool>, Query, description = "download")),
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
    path = "/templates/trigger/import",
    tag = "template",
    request_body = serde_json::Value,
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
            "/api/v1/templates/agent-trigger",
            "/api/v1/templates/agent-trigger/summaries",
            "/api/v1/templates/agent",
            "/api/v1/templates/agent/summaries",
            "/api/v1/templates/agent/featured",
            "/api/v1/templates/agent/popular",
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
            "/api/v1/templates/library/workflows/missing",
            "/api/v1/templates/library/agents/missing",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
        // The library registries are queryable even when empty.
        for uri in [
            "/api/v1/templates/library/workflows",
            "/api/v1/templates/library/agents",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
    }
}

//! Template domain: node / trigger template CRUD with export-import. Agent
//! trigger / agent template query surfaces live in `api_template_queries` and
//! the shared template library in `api_template_library`.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use wf_storage::adapter::node_template::NodeTemplateListOptions;
use wf_types::{NodeTemplateStorageMetadata, TriggerTemplateStorageMetadata};

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .merge(crate::api::resource::template_queries::routes())
        .merge(crate::api::resource::template_library::routes())
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
struct ListNodeTemplatesQuery {
    #[serde(flatten)]
    page: ListQuery,
    node_type: Option<String>,
}

async fn handle_list_node_templates(
    State(state): State<ApiState>,
    Query(query): Query<ListNodeTemplatesQuery>,
) -> impl IntoResponse {
    let options = NodeTemplateListOptions {
        offset: query.page.offset,
        limit: query.page.limit,
        node_type_filter: query.node_type,
    };
    match wf_api::template::node_template::node_template_summaries(
        &state.ctx.storage,
        Some(options),
    )
    .await
    {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_node_template(
    State(state): State<ApiState>,
    Json(template): Json<NodeTemplateStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::template::node_template::save_node_template(&state.ctx.storage, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::node_template::get_node_template(&state.ctx.storage, &path.id).await {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_update_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut template): Json<NodeTemplateStorageMetadata>,
) -> impl IntoResponse {
    template.id = wf_types::Id::from(path.id.clone());
    match wf_api::template::node_template::save_node_template(&state.ctx.storage, &template).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::node_template::delete_node_template(&state.ctx.storage, &path.id).await
    {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_export_node_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::node_template::export_template(&state.ctx.storage, &path.id).await {
        Ok(json) => ok(json).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ImportBody {
    json: String,
}

async fn handle_import_node_template(
    State(state): State<ApiState>,
    Json(body): Json<ImportBody>,
) -> impl IntoResponse {
    match wf_api::template::node_template::import_template(&state.ctx.storage, &body.json).await {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

// ── trigger templates ─────────────────────────────────────────────

#[derive(Deserialize)]
struct ListTriggerTemplatesQuery {
    trigger_type: Option<String>,
}

async fn handle_list_trigger_templates(
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
    match wf_api::template::agent_trigger_template::summaries(&state.ctx, Some(&filter)).await {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_save_trigger_template(
    State(state): State<ApiState>,
    Json(template): Json<TriggerTemplateStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::template::agent_trigger_template::save(&state.ctx, &template).await {
        Ok(()) => ok(template.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_trigger_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::agent_trigger_template::get(&state.ctx, &path.id).await {
        Ok(template) => ok(template).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_update_trigger_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut template): Json<TriggerTemplateStorageMetadata>,
) -> impl IntoResponse {
    template.id = wf_types::Id::from(path.id.clone());
    match wf_api::template::agent_trigger_template::save(&state.ctx, &template).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_trigger_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::agent_trigger_template::delete(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_export_trigger_template(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::template::agent_trigger_template::export_template(&state.ctx, &path.id).await {
        Ok(json) => ok(json).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_import_trigger_template(
    State(state): State<ApiState>,
    Json(body): Json<ImportBody>,
) -> impl IntoResponse {
    match wf_api::template::agent_trigger_template::import_template(&state.ctx, &body.json).await {
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
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
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

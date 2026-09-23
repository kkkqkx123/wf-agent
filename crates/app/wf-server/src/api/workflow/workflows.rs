//! Workflow domain: CRUD, import/export, validation, clone, summaries,
//! search and metadata updates. Version routes live in
//! `workflow/versions` and graph query routes in `workflow/graphs`;
//! every handler is a thin transport adapter over the `wf-api::workflow`
//! surface, errors map through the shared envelope.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use wf_api::WorkflowDefinition;
use wf_api::WorkflowListOptions;

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery, NamePath};
use crate::paged::{fetch_size, ok_page, resolve_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/workflows",
            get(handle_list_workflows).post(handle_create_workflow),
        )
        .route(
            "/workflows/{id}",
            get(handle_get_workflow)
                .put(handle_update_workflow)
                .delete(handle_delete_workflow),
        )
        .route("/workflows/{id}/clone", post(handle_clone_workflow))
        .route("/workflows/validate", post(handle_validate_workflow))
        .route("/workflows/validate/node", post(handle_validate_node))
        .route("/workflows/parse", post(handle_parse_workflow))
        .route("/workflows/transform", post(handle_transform_workflow))
        .route("/workflows/summaries", get(handle_workflow_summaries))
        .route("/workflows/search", get(handle_search_workflows))
        .route("/workflows/by-name/{name}", get(handle_workflow_by_name))
        .route("/workflows/by-tags", get(handle_workflows_by_tags))
        .route(
            "/workflows/by-category/{category}",
            get(handle_workflows_by_category),
        )
        .route(
            "/workflows/by-author/{author}",
            get(handle_workflows_by_author),
        )
        .route("/workflows/export-all", post(handle_export_workflows))
        .route("/workflows/{id}/export", get(handle_export_workflow))
        .route("/workflows/import", post(handle_import_workflow))
        .route("/workflows/import-many", post(handle_import_many))
        .route("/workflows/{id}/metadata", patch(handle_update_metadata))
}

#[derive(Deserialize)]
struct ListWorkflowsQuery {
    #[serde(flatten)]
    page: ListQuery,
    name: Option<String>,
    r#type: Option<String>,
}

async fn handle_list_workflows(
    State(state): State<ApiState>,
    Query(query): Query<ListWorkflowsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    let options = WorkflowListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        name_filter: query.name,
        type_filter: query.r#type,
    };
    match wf_api::workflow::list_workflows(&state.ctx, Some(options)).await {
        Ok(workflows) => ok_page(workflows, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_create_workflow(
    State(state): State<ApiState>,
    Json(workflow): Json<WorkflowDefinition>,
) -> impl IntoResponse {
    match wf_api::workflow::save_workflow(&state.ctx, &workflow).await {
        Ok(()) => ok(workflow.id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_update_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut workflow): Json<WorkflowDefinition>,
) -> impl IntoResponse {
    workflow.id = path.id;
    match wf_api::workflow::save_workflow(&state.ctx, &workflow).await {
        Ok(()) => ok(workflow.id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::delete_workflow(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::get_workflow(&state.ctx, &path.id).await {
        Ok(workflow) => ok(workflow).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct CloneBody {
    new_id: Option<String>,
}

async fn handle_clone_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<CloneBody>,
) -> impl IntoResponse {
    match wf_api::workflow::clone_workflow(&state.ctx, &path.id, body.new_id.as_deref()).await {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_validate_workflow(
    State(_state): State<ApiState>,
    Json(workflow): Json<WorkflowDefinition>,
) -> impl IntoResponse {
    match wf_api::workflow::validate_workflow(&workflow) {
        Ok(()) => ok(true).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateNodeBody {
    node_type: String,
    node_id: String,
    config: Option<Value>,
}

/// Validate a single node config by node type through the wf-config
/// processor, mirroring `wf-api::infra::config::validate_node`.
async fn handle_validate_node(
    State(_state): State<ApiState>,
    Json(body): Json<ValidateNodeBody>,
) -> impl IntoResponse {
    match wf_api::infra::config::validate_node(&body.node_type, &body.node_id, body.config.as_ref())
    {
        Ok(()) => ok(true).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ParseWorkflowBody {
    format: Option<String>,
    content: String,
}

/// Parse a workflow definition from JSON or TOML text without persisting it.
async fn handle_parse_workflow(
    State(_state): State<ApiState>,
    Json(body): Json<ParseWorkflowBody>,
) -> impl IntoResponse {
    let format = match body.format.as_deref() {
        None | Some("json") => wf_api::infra::config::ConfigFormat::Json,
        Some("toml") => wf_api::infra::config::ConfigFormat::Toml,
        Some(other) => {
            return crate::envelope::err::<Value>(crate::envelope::ApiError::validation(format!(
                "unsupported config format: {other}"
            )))
            .into_response()
        }
    };
    match wf_api::infra::config::parse_workflow(&body.content, format) {
        Ok(workflow) => ok(workflow).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct TransformWorkflowBody {
    nodes: Vec<wf_api::infra::config::WorkflowNodeConfig>,
    edges: Vec<wf_api::infra::config::WorkflowEdgeConfig>,
}

#[derive(Serialize)]
struct TransformWorkflowView {
    nodes: Vec<wf_api::BaseStaticNode>,
    edges: Vec<wf_api::Edge>,
}

/// Convert declarative node/edge configs into canonical runtime structures.
async fn handle_transform_workflow(
    State(_state): State<ApiState>,
    Json(body): Json<TransformWorkflowBody>,
) -> impl IntoResponse {
    let nodes = match wf_api::infra::config::transform_workflow_nodes(&body.nodes) {
        Ok(nodes) => nodes,
        Err(e) => return error_response(e),
    };
    let edges = match wf_api::infra::config::transform_workflow_edges(&body.edges) {
        Ok(edges) => edges,
        Err(e) => return error_response(e),
    };
    ok(TransformWorkflowView { nodes, edges }).into_response()
}

async fn handle_workflow_summaries(
    State(state): State<ApiState>,
    Query(query): Query<ListWorkflowsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    let options = WorkflowListOptions {
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
        name_filter: query.name,
        type_filter: query.r#type,
    };
    match wf_api::workflow::workflow_summaries(&state.ctx, Some(options)).await {
        Ok(summaries) => ok_page(summaries, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ExportWorkflowQuery {
    format: Option<String>,
    download: Option<bool>,
}

async fn handle_export_workflow(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ExportWorkflowQuery>,
) -> impl IntoResponse {
    let download = query.download.unwrap_or(false);
    match query.format.as_deref() {
        None | Some("json") => {
            match wf_api::workflow::export_workflow_json(&state.ctx, &path.id).await {
                Ok(json) => {
                    if download {
                        crate::envelope::download(
                            &json,
                            "application/json",
                            &format!("workflow-{}.json", path.id),
                        )
                        .into_response()
                    } else {
                        ok(json).into_response()
                    }
                }
                Err(e) => error_response(e),
            }
        }
        Some("toml") => match wf_api::workflow::get_workflow(&state.ctx, &path.id).await {
            Ok(workflow) => match wf_api::infra::config::export_toml(&workflow) {
                Ok(toml) => {
                    if download {
                        crate::envelope::download(
                            &toml,
                            "application/toml",
                            &format!("workflow-{}.toml", path.id),
                        )
                        .into_response()
                    } else {
                        ok(toml).into_response()
                    }
                }
                Err(e) => error_response(e),
            },
            Err(e) => error_response(e),
        },
        Some(other) => crate::envelope::err::<Value>(crate::envelope::ApiError::validation(
            format!("unsupported export format: {other}"),
        ))
        .into_response(),
    }
}

#[derive(Deserialize)]
struct SearchWorkflowsQuery {
    q: Option<String>,
    tags: Option<String>,
    category: Option<String>,
    author: Option<String>,
    #[serde(flatten)]
    page: ListQuery,
}

async fn handle_search_workflows(
    State(state): State<ApiState>,
    Query(query): Query<SearchWorkflowsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    let options = wf_api::workflow::WorkflowSearchOptions {
        keyword: query.q,
        tags: query.tags.as_deref().map(|raw| {
            raw.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        }),
        category: query.category,
        author: query.author,
        offset: Some(offset),
        limit: Some(fetch_size(limit)),
    };
    match wf_api::workflow::search_workflows(&state.ctx, &options).await {
        Ok(workflows) => ok_page(workflows, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_workflow_by_name(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
) -> impl IntoResponse {
    match wf_api::workflow::get_workflow_by_name(&state.ctx, &path.name).await {
        Ok(workflow) => ok(workflow).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_workflows_by_tags(
    State(state): State<ApiState>,
    Query(query): Query<SearchWorkflowsQuery>,
) -> impl IntoResponse {
    let tags: Vec<String> = query
        .tags
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    match wf_api::workflow::get_workflows_by_tags(&state.ctx, &tags).await {
        Ok(workflows) => {
            let (limit, offset) = resolve_page(&query.page);
            let window = workflows
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CategoryPath {
    category: String,
}

async fn handle_workflows_by_category(
    State(state): State<ApiState>,
    Path(path): Path<CategoryPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::workflow::get_workflows_by_category(&state.ctx, &path.category).await {
        Ok(workflows) => {
            let (limit, offset) = resolve_page(&query);
            let window = workflows
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorPath {
    author: String,
}

async fn handle_workflows_by_author(
    State(state): State<ApiState>,
    Path(path): Path<AuthorPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::workflow::get_workflows_by_author(&state.ctx, &path.author).await {
        Ok(workflows) => {
            let (limit, offset) = resolve_page(&query);
            let window = workflows
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ExportManyBody {
    ids: Vec<String>,
}

async fn handle_export_workflows(
    State(state): State<ApiState>,
    Json(body): Json<ExportManyBody>,
) -> impl IntoResponse {
    match wf_api::workflow::export_workflows(&state.ctx, &body.ids).await {
        Ok(export) => ok(export).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ImportBody {
    json: String,
    new_id: Option<String>,
}

async fn handle_import_workflow(
    State(state): State<ApiState>,
    Json(body): Json<ImportBody>,
) -> impl IntoResponse {
    match wf_api::workflow::import_workflow_json(&state.ctx, &body.json, body.new_id.as_deref())
        .await
    {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_import_many(
    State(state): State<ApiState>,
    Json(json): Json<serde_json::Value>,
) -> impl IntoResponse {
    match wf_api::workflow::import_workflows(&state.ctx, &json).await {
        Ok(ids) => ok(ids).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_update_metadata(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(metadata): Json<HashMap<String, serde_json::Value>>,
) -> impl IntoResponse {
    match wf_api::workflow::update_workflow_metadata(&state.ctx, &path.id, &metadata).await {
        Ok(()) => ok(metadata).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::Request;
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

    async fn get(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn sample_workflow(id: &str) -> wf_types::WorkflowDefinition {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": format!("Workflow {id}"),
            "version": "1.0.0",
            "nodes": [
                {"id": "start", "node_type": "START", "name": "start"},
                {"id": "end", "node_type": "END", "name": "end"}
            ],
            "edges": [
                {"id": "e1", "source_node_id": "start", "target_node_id": "end", "type": "DEFAULT"}
            ],
            "created_at": 1000,
            "updated_at": 1000
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn workflow_crud_roundtrip() {
        let ctx = make_ctx();

        let create = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows")
                    .header("content-type", "application/json")
                    .body(AxBody::from(
                        serde_json::to_vec(&sample_workflow("wf-crud")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create.status(), axum::http::StatusCode::OK);

        let listed = get(ctx.clone(), "/api/v1/workflows").await;
        assert_eq!(listed.status(), axum::http::StatusCode::OK);
        let body = json_body(listed).await;
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["has_more"], false);

        let exported = get(ctx.clone(), "/api/v1/workflows/wf-crud/export").await;
        assert_eq!(exported.status(), axum::http::StatusCode::OK);
        let body = json_body(exported).await;
        assert!(body["data"].as_str().unwrap().contains("wf-crud"));

        let deleted = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/workflows/wf-crud")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(deleted.status(), axum::http::StatusCode::OK);
        let body = json_body(deleted).await;
        assert_eq!(body["data"], true);
    }
}

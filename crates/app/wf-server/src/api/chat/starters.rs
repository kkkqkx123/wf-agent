//! Chat starter prompts: suggestion chips of an empty chat session.
//!
//! Read surface (`list` / `detail` / `usage`) feeds the composer; write
//! surface (create / update / delete / enable / disable / import / export)
//! backs the template-library management tab. Builtin rows stay read-only
//! except for the enable flag and the usage counter.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::paged::{fetch_size, ok_page, resolve_page_fields};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/chat/starters",
            get(handle_list_starters).post(handle_create_starter),
        )
        .route(
            "/chat/starters/{id}",
            get(handle_get_starter)
                .put(handle_update_starter)
                .delete(handle_delete_starter),
        )
        .route("/chat/starters/{id}/enable", post(handle_enable_starter))
        .route("/chat/starters/{id}/disable", post(handle_disable_starter))
        .route(
            "/chat/starters/{id}/usage",
            post(handle_record_starter_usage),
        )
        .route("/chat/starters/import", post(handle_import_starter))
        .route("/chat/starters/{id}/export", get(handle_export_starter))
        .route("/chat/starters/{id}/resolve", post(handle_resolve_starter))
}

/// Wire view of one starter. The domain type stays out of the OpenAPI
/// surface; only the fields the web frontend consumes are schemad.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct StarterView {
    id: String,
    title: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<serde_json::Value>>)]
    variables: Option<Vec<wf_types::TemplateVariableDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    locale: String,
    tags: Vec<String>,
    enabled: bool,
    is_default: bool,
    is_system: bool,
    usage_count: u64,
    created_at: String,
    updated_at: String,
}

impl From<wf_api::chat::starter::StarterEntry> for StarterView {
    fn from(entry: wf_api::chat::starter::StarterEntry) -> Self {
        Self {
            id: entry.id,
            title: entry.title,
            content: entry.content,
            variables: entry.variables,
            description: entry.description,
            locale: entry.locale,
            tags: entry.tags,
            enabled: entry.enabled,
            is_default: entry.is_default,
            is_system: entry.is_system,
            usage_count: entry.usage_count,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListStartersQuery {
    locale: Option<String>,
    q: Option<String>,
    tags: Option<String>,
    enabled: Option<bool>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/chat/starters",
    tag = "chat",
    params(ListStartersQuery),
    responses((status = 200, description = "Paged starter prompts", body = crate::envelope::ApiEnvelope<crate::paged::PageView<StarterView>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_starters(
    State(state): State<ApiState>,
    Query(query): Query<ListStartersQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page_fields(query.limit, query.offset);
    let tags: Vec<String> = query
        .tags
        .as_deref()
        .map(|t| t.split(',').map(ToOwned::to_owned).collect())
        .unwrap_or_default();
    match wf_api::chat::starter::list(
        &state.ctx,
        query.locale.as_deref(),
        query.q.as_deref(),
        &tags,
        query.enabled.unwrap_or(true),
    )
    .await
    {
        Ok(entries) => {
            let window: Vec<StarterView> = entries
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .map(StarterView::from)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/chat/starters/{id}",
    tag = "chat",
    params(IdPath),
    responses((status = 200, description = "Starter detail", body = crate::envelope::ApiEnvelope<StarterView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_starter(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::chat::starter::get(&state.ctx, &path.id).await {
        Ok(entry) => ok(StarterView::from(entry)).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateStarterBody {
    #[schema(value_type = Option<String>)]
    id: Option<String>,
    title: String,
    content: String,
    #[schema(value_type = Option<Vec<serde_json::Value>>)]
    variables: Option<Vec<wf_types::TemplateVariableDefinition>>,
    #[schema(value_type = Option<String>)]
    description: Option<String>,
    #[schema(value_type = Option<String>)]
    locale: Option<String>,
    tags: Option<Vec<String>>,
    enabled: Option<bool>,
    is_default: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/starters",
    tag = "chat",
    request_body = CreateStarterBody,
    responses((status = 200, description = "Created starter", body = crate::envelope::ApiEnvelope<StarterView>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_create_starter(
    State(state): State<ApiState>,
    Json(body): Json<CreateStarterBody>,
) -> impl IntoResponse {
    let input = wf_api::chat::starter::CreateStarterInput {
        id: body.id,
        title: body.title,
        content: body.content,
        variables: body.variables,
        description: body.description,
        locale: body.locale,
        tags: body.tags,
        enabled: body.enabled,
        is_default: body.is_default,
    };
    match wf_api::chat::starter::create(&state.ctx, input).await {
        Ok(entry) => ok(StarterView::from(entry)).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct UpdateStarterBody {
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    title: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    content: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<Vec<serde_json::Value>>)]
    variables: Option<Option<Vec<wf_types::TemplateVariableDefinition>>>,
    #[serde(default)]
    description: Option<Option<String>>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    locale: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    is_default: Option<bool>,
}

#[utoipa::path(
    put,
    path = "/api/v1/chat/starters/{id}",
    tag = "chat",
    params(IdPath),
    request_body = UpdateStarterBody,
    responses((status = 200, description = "Updated starter", body = crate::envelope::ApiEnvelope<StarterView>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_starter(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<UpdateStarterBody>,
) -> impl IntoResponse {
    let input = wf_api::chat::starter::UpdateStarterInput {
        title: body.title,
        content: body.content,
        variables: body.variables,
        description: body.description,
        locale: body.locale,
        tags: body.tags,
        enabled: body.enabled,
        is_default: body.is_default,
    };
    match wf_api::chat::starter::update(&state.ctx, &path.id, input).await {
        Ok(entry) => ok(StarterView::from(entry)).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/chat/starters/{id}",
    tag = "chat",
    params(IdPath),
    responses((status = 200, description = "Deletion flag", body = crate::envelope::ApiEnvelope<bool>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Builtin starters cannot be deleted", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_starter(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::chat::starter::delete(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/starters/{id}/enable",
    tag = "chat",
    params(IdPath),
    responses((status = 200, description = "Enabled starter", body = crate::envelope::ApiEnvelope<StarterView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_enable_starter(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::chat::starter::set_enabled(&state.ctx, &path.id, true).await {
        Ok(entry) => ok(StarterView::from(entry)).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/starters/{id}/disable",
    tag = "chat",
    params(IdPath),
    responses((status = 200, description = "Disabled starter", body = crate::envelope::ApiEnvelope<StarterView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_disable_starter(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::chat::starter::set_enabled(&state.ctx, &path.id, false).await {
        Ok(entry) => ok(StarterView::from(entry)).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/starters/{id}/usage",
    tag = "chat",
    params(IdPath),
    responses((status = 200, description = "Updated usage count", body = crate::envelope::ApiEnvelope<u64>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_record_starter_usage(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::chat::starter::record_usage(&state.ctx, &path.id).await {
        Ok(count) => ok(count).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct StarterExportQuery {
    download: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/v1/chat/starters/{id}/export",
    tag = "chat",
    params(IdPath, StarterExportQuery),
    responses((status = 200, description = "Exported starter document", body = String, content_type = "application/json"), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_export_starter(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<StarterExportQuery>,
) -> impl IntoResponse {
    match wf_api::chat::starter::export_json(&state.ctx, &path.id).await {
        Ok(json) => {
            if query.download.unwrap_or(false) {
                crate::envelope::download(
                    &json,
                    "application/json",
                    &format!("chat-starter-{}.json", path.id),
                )
                .into_response()
            } else {
                ok(json).into_response()
            }
        }
        Err(e) => error_response(e),
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ResolveStarterView {
    content: String,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct ResolveStarterBody {
    #[schema(value_type = Object)]
    values: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ResolveStarterMissing {
    missing: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/starters/{id}/resolve",
    tag = "chat",
    params(IdPath),
    request_body = ResolveStarterBody,
    responses((status = 200, description = "Resolved starter content", body = crate::envelope::ApiEnvelope<ResolveStarterView>), (status = 400, description = "Missing variables or invalid values", body = crate::envelope::ErrorResponse), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_resolve_starter(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<ResolveStarterBody>,
) -> impl IntoResponse {
    match wf_api::chat::starter::get(&state.ctx, &path.id).await {
        Ok(entry) => match wf_api::chat::starter::resolve_starter_for_fill(&entry, &body.values) {
            Ok(content) => ok(ResolveStarterView { content }).into_response(),
            Err(wf_api::chat::starter::StarterFillError::Missing(missing)) => {
                crate::envelope::error_response(wf_api::ApiError::Validation(format!(
                    "starter [{id}] is missing variable(s): {missing}",
                    id = path.id,
                    missing = missing.join(", ")
                )))
            }
            Err(wf_api::chat::starter::StarterFillError::Invalid(reason)) => {
                crate::envelope::error_response(wf_api::ApiError::Validation(reason))
            }
        },
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct ImportStarterBody {
    json: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/starters/import",
    tag = "chat",
    request_body = ImportStarterBody,
    responses((status = 200, description = "Imported starter", body = crate::envelope::ApiEnvelope<StarterView>), (status = 400, description = "Invalid starter document", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_starter(
    State(state): State<ApiState>,
    Json(body): Json<ImportStarterBody>,
) -> impl IntoResponse {
    match wf_api::chat::starter::import_json(&state.ctx, &body.json).await {
        Ok(entry) => ok(StarterView::from(entry)).into_response(),
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

    async fn send(ctx: Arc<ApiContext>, request: Request<AxBody>) -> Response {
        crate::router::api_router(ctx)
            .oneshot(request)
            .await
            .unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn get(uri: &str) -> Request<AxBody> {
        Request::builder().uri(uri).body(AxBody::empty()).unwrap()
    }

    fn post(uri: &str, body: &str) -> Request<AxBody> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(AxBody::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn starter_list_is_paged_and_seeded() {
        let ctx = make_ctx();
        let response = send(ctx, get("/api/v1/chat/starters?locale=en")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let items = body["data"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert!(items.iter().all(|item| item["locale"] == "en"));
    }

    #[tokio::test]
    async fn starter_crud_roundtrip_with_builtin_guard() {
        let ctx = make_ctx();
        // Builtin detail resolves.
        let response = send(ctx.clone(), get("/api/v1/chat/starters/starter.plan-en")).await;
        assert_eq!(response.status(), StatusCode::OK);
        // Builtin edit is rejected.
        let response = send(
            ctx.clone(),
            Request::builder()
                .method("PUT")
                .uri("/api/v1/chat/starters/starter.plan-en")
                .header("content-type", "application/json")
                .body(AxBody::from(r#"{"title":"hijack"}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        // Create a custom starter.
        let response = send(
            ctx.clone(),
            post(
                "/api/v1/chat/starters",
                r#"{"title":"Demo","content":"run the demo","locale":"en","tags":["demo"]}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let created = json_body(response).await;
        let id = created["data"]["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("starter.custom."));
        // Disable hides it from the default (enabled-only) list.
        let response = send(
            ctx.clone(),
            post(&format!("/api/v1/chat/starters/{id}/disable"), ""),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = send(ctx.clone(), get("/api/v1/chat/starters?locale=en")).await;
        let listed = json_body(response).await;
        assert!(!listed["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == id));
        // Usage still counts a disabled custom row.
        let response = send(
            ctx.clone(),
            post(&format!("/api/v1/chat/starters/{id}/usage"), ""),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        // Delete removes it.
        let response = send(
            ctx.clone(),
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/chat/starters/{id}"))
                .body(AxBody::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let response = send(ctx, get(&format!("/api/v1/chat/starters/{id}"))).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn starter_import_export_roundtrip() {
        let ctx = make_ctx();
        let response = send(
            ctx.clone(),
            get("/api/v1/chat/starters/starter.tools-en/export"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let exported = json_body(response).await;
        let doc = exported["data"].as_str().unwrap();
        let response = send(
            ctx.clone(),
            post(
                "/api/v1/chat/starters/import",
                &serde_json::to_string(&serde_json::json!({ "json": doc })).unwrap(),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let imported = json_body(response).await;
        assert_eq!(imported["data"]["is_system"], false);
        assert_eq!(
            imported["data"]["title"],
            "List the available tools and skills"
        );
    }

    #[tokio::test]
    async fn starter_unknown_id_maps_to_not_found() {
        let ctx = make_ctx();
        let response = send(ctx, get("/api/v1/chat/starters/starter.missing")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn starter_resolve_reports_missing_then_renders() {
        let ctx = make_ctx();
        let response = send(
            ctx.clone(),
            post(
                "/api/v1/chat/starters",
                r#"{"title":"Fill","content":"Do {{task}}","locale":"en","variables":[{"name":"task","type":"string","required":true}]}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let created = json_body(response).await;
        let id = created["data"]["id"].as_str().unwrap().to_string();
        let response = send(
            ctx.clone(),
            post(
                &format!("/api/v1/chat/starters/{id}/resolve"),
                r#"{"values":{}}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let response = send(
            ctx.clone(),
            post(
                &format!("/api/v1/chat/starters/{id}/resolve"),
                r#"{"values":{"task":"the thing"}}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let resolved = json_body(response).await;
        assert_eq!(resolved["data"]["content"], "Do the thing");
    }
}

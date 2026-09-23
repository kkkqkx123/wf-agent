//! LLM domain: direct generation (single/batch/stream/token-count) and LLM
//! profile management (CRUD / default / export-import / templates). Script
//! and tool registries live in the sibling modules `scripts` and `tools`.

use std::convert::Infallible;
use utoipa::{IntoParams, ToSchema};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_api::{LlmProfile, LlmRequest};

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, NamePath};
use crate::paged::{fetch_size, ok_page, resolve_page_fields};
use crate::router::ApiState;
use crate::sse::sse_response;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── LLM generation ──
        .route("/llm/generate", post(handle_generate))
        .route("/llm/generate-batch", post(handle_generate_batch))
        .route("/llm/generate-stream", post(handle_generate_stream))
        .route("/llm/count-tokens", post(handle_count_tokens))
        // ── LLM profiles ──
        .route(
            "/llm/profiles",
            get(handle_list_profiles).post(handle_create_profile),
        )
        .route(
            "/llm/profiles/{id}",
            get(handle_get_profile)
                .put(handle_update_profile)
                .delete(handle_delete_profile),
        )
        .route("/llm/profiles/{id}/default", post(handle_set_default))
        .route("/llm/profiles/default", get(handle_get_default))
        .route("/llm/profiles/{id}/export", get(handle_export_profile))
        .route("/llm/profiles/import", post(handle_import_profile))
        .route("/llm/profiles/export-all", get(handle_export_all_profiles))
        .route("/llm/profiles/import-all", post(handle_import_all_profiles))
        .route(
            "/llm/profile-templates",
            get(handle_list_templates).post(handle_add_template),
        )
        .route("/llm/profile-templates", delete(handle_remove_template))
        .route(
            "/llm/profile-templates/{name}",
            get(handle_get_template_by_name),
        )
        .route("/llm/profiles/validate", post(handle_validate_profile))
        .route(
            "/llm/profiles/from-template",
            post(handle_create_from_template),
        )
        // ── LLM providers ──
        .route(
            "/llm/providers",
            get(handle_list_providers).post(handle_create_provider),
        )
        .route(
            "/llm/providers/{id}",
            get(handle_get_provider).delete(handle_delete_provider),
        )
        .route("/llm/providers/{id}/models", get(handle_list_models))
}

// ── LLM generation ────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/llm/generate",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_generate(
    State(state): State<ApiState>,
    Json(request): Json<LlmRequest>,
) -> impl IntoResponse {
    match wf_api::llm::generate(&state.ctx, &request).await {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/generate-batch",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_generate_batch(
    State(state): State<ApiState>,
    Json(requests): Json<Vec<LlmRequest>>,
) -> impl IntoResponse {
    match wf_api::llm::generate_batch(&state.ctx, &requests).await {
        Ok(results) => ok(results).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/generate-stream",
    tag = "llm",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Server-sent events stream", content_type = "text/event-stream"),
        (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_generate_stream(
    State(state): State<ApiState>,
    Json(request): Json<LlmRequest>,
) -> Response {
    let stream = match wf_api::llm::generate_stream(&state.ctx, &request).await {
        Ok(stream) => stream,
        Err(e) => return error_response(e),
    };
    let events = futures::stream::unfold(stream, |mut stream| async move {
        match stream.next().await {
            Some(Ok(event)) => {
                let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
                let frame = format!("data: {payload}\n\n");
                Some((Ok::<_, Infallible>(Bytes::from(frame)), stream))
            }
            Some(Err(err)) => {
                let frame = format!(
                    "data: {{\"event_type\":\"error\",\"error\":\"{}\"}}\n\n",
                    err
                );
                Some((Ok::<_, Infallible>(Bytes::from(frame)), stream))
            }
            None => None,
        }
    });
    sse_response(events)
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/count-tokens",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_count_tokens(
    State(state): State<ApiState>,
    Json(request): Json<LlmRequest>,
) -> impl IntoResponse {
    match wf_api::llm::count_tokens(&state.ctx, &request).await {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

// ── LLM profiles ──────────────────────────────────────────────────

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListProfilesQuery {
    id: Option<String>,
    name: Option<String>,
    format: Option<String>,
    model: Option<String>,
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/profiles",
    tag = "llm",
    params(ListProfilesQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_profiles(
    State(state): State<ApiState>,
    Query(query): Query<ListProfilesQuery>,
) -> impl IntoResponse {
    let filter = wf_api::LlmProfileFilter {
        id: query.id,
        name: query.name,
        format: query
            .format
            .as_deref()
            .and_then(|p| serde_json::from_value(serde_json::json!(p)).ok()),
        model: query.model,
    };
    match wf_api::llm::llm_profile::query(&state.ctx, &filter).await {
        Ok(profiles) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
            let window = profiles
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
    path = "/api/v1/llm/profiles",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_create_profile(
    State(state): State<ApiState>,
    Json(profile): Json<wf_api::LlmProfile>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::create(&state.ctx, &profile).await {
        Ok(()) => ok(profile.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/profiles/{id}",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::get(&state.ctx, &path.id).await {
        Ok(profile) => ok(profile).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/llm/profiles/{id}",
    tag = "llm",
    params(IdPath),
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut profile): Json<wf_api::LlmProfile>,
) -> impl IntoResponse {
    profile.id = path.id;
    match wf_api::llm::llm_profile::update(&state.ctx, &profile).await {
        Ok(()) => ok(profile.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/llm/profiles/{id}",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::delete(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/profiles/{id}/default",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_set_default(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::set_default(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/profiles/default",
    tag = "llm",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_default(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::llm_profile::get_default(&state.ctx).await {
        Ok(profile) => ok(profile).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/profiles/{id}/export",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_export_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::export_json(&state.ctx, &path.id).await {
        Ok(json) => ok(json).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct ImportProfileBody {
    json: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/profiles/import",
    tag = "llm",
    request_body = ImportProfileBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_profile(
    State(state): State<ApiState>,
    Json(body): Json<ImportProfileBody>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::import_json(&state.ctx, &body.json).await {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/profiles/export-all",
    tag = "llm",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_export_all_profiles(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::llm_profile::export_all_json(&state.ctx).await {
        Ok(json) => ok(json).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/profiles/import-all",
    tag = "llm",
    request_body = ImportProfileBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_import_all_profiles(
    State(state): State<ApiState>,
    Json(body): Json<ImportProfileBody>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::import_all_json(&state.ctx, &body.json).await {
        Ok(ids) => ok(ids).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/profile-templates",
    tag = "llm",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_templates(State(state): State<ApiState>) -> impl IntoResponse {
    // Bounded in-memory catalog (built-in plus custom templates); retained as
    // a bare array with no pagination.
    match wf_api::llm::llm_profile::list_templates(&state.ctx).await {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/profile-templates",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_add_template(
    State(state): State<ApiState>,
    Json(template): Json<wf_api::LlmProfileTemplate>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::add_template(&state.ctx, template).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct TemplateNameQuery {
    name: String,
}

#[utoipa::path(
    delete,
    path = "/api/v1/llm/profile-templates",
    tag = "llm",
    params(TemplateNameQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_remove_template(
    State(state): State<ApiState>,
    Query(query): Query<TemplateNameQuery>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::remove_template(&state.ctx, &query.name).await {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

/// Single profile template by name (built-in or custom).
#[utoipa::path(
    get,
    path = "/api/v1/llm/profile-templates/{name}",
    tag = "llm",
    params(NamePath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_template_by_name(
    State(state): State<ApiState>,
    Path(path): Path<NamePath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::get_template(&state.ctx, &path.name).await {
        Ok(Some(template)) => ok(template).into_response(),
        Ok(None) => error_response(wf_api::ApiError::not_found("template", &path.name)),
        Err(e) => error_response(e),
    }
}

/// Validate an LLM profile without persisting it.
#[utoipa::path(
    post,
    path = "/api/v1/llm/profiles/validate",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_validate_profile(
    State(state): State<ApiState>,
    Json(profile): Json<LlmProfile>,
) -> impl IntoResponse {
    let (valid, errors) = wf_api::llm::llm_profile::validate(&state.ctx, &profile);
    ok(serde_json::json!({ "valid": valid, "errors": errors })).into_response()
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateFromTemplateBody {
    template_name: String,
    overrides: Value,
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/profiles/from-template",
    tag = "llm",
    request_body = CreateFromTemplateBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_create_from_template(
    State(state): State<ApiState>,
    Json(body): Json<CreateFromTemplateBody>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::create_from_template(
        &state.ctx,
        &body.template_name,
        &body.overrides,
    )
    .await
    {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

// ── LLM providers ─────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/llm/providers",
    tag = "llm",
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_providers(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::llm_provider::list(&state.ctx).await {
        Ok(providers) => ok(providers).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/llm/providers",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_create_provider(
    State(state): State<ApiState>,
    Json(provider): Json<wf_api::LlmProviderDefinition>,
) -> impl IntoResponse {
    match wf_api::llm::llm_provider::create(&state.ctx, &provider).await {
        Ok(()) => ok(provider.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/llm/providers/{id}",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_provider(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_provider::get(&state.ctx, &path.id).await {
        Ok(provider) => ok(provider).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/llm/providers/{id}",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_provider(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_provider::delete(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

/// Off-hot-path model listing for a provider definition.
#[utoipa::path(
    get,
    path = "/api/v1/llm/providers/{id}/models",
    tag = "llm",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_models(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_provider::list_models(&state.ctx, &path.id).await {
        Ok(models) => ok(models).into_response(),
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
    async fn llm_profile_and_registry_endpoints_are_reachable() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/llm/profiles",
            "/api/v1/llm/profiles/default",
            "/api/v1/llm/profile-templates",
            "/api/v1/llm/providers",
            "/api/v1/scripts",
            "/api/v1/scripts/search?q=test",
            "/api/v1/tools",
            "/api/v1/tools/search?q=bash",
            "/api/v1/tool-registry",
            "/api/v1/tool-registry/stats",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }

        // Single profile template by name resolves to a built-in template.
        let template = send(ctx.clone(), "/api/v1/llm/profile-templates/openai-chat").await;
        assert_eq!(template.status(), StatusCode::OK, "template by name");
        let unknown = send(ctx.clone(), "/api/v1/llm/profile-templates/does-not-exist").await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND, "unknown template");

        // Profile validation reports errors without persisting.
        let validated = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/llm/profiles/validate")
                    .header("content-type", "application/json")
                    .body(AxBody::from(
                        serde_json::json!({
                            "id": "",
                            "name": "",
                            "model": "",
                            "format": "OPENAI_CHAT"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(validated.status(), StatusCode::OK, "validate profile");
        let body = json_body(validated).await;
        assert_eq!(body["data"]["valid"], false);
        assert!(!body["data"]["errors"].as_array().unwrap().is_empty());
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn llm_generate_rejects_empty_request() {
        let ctx = make_ctx();
        let response = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/llm/generate")
                    .header("content-type", "application/json")
                    .body(AxBody::from(
                        serde_json::to_vec(&serde_json::json!({
                            "profile_id": "p1",
                            "messages": []
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn script_execute_rejects_empty_name() {
        let ctx = make_ctx();
        let response = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/scripts/execute")
                    .header("content-type", "application/json")
                    .body(AxBody::from(
                        serde_json::to_vec(&serde_json::json!({"name": ""})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
pub mod scripts;
pub mod tools;

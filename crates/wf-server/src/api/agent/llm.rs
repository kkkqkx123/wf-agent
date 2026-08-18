//! LLM domain: direct generation (single/batch/stream/token-count) and LLM
//! profile management (CRUD / default / export-import / templates). Script
//! and tool registries live in the sibling modules `api_scripts` and
//! `api_tools`.

use std::convert::Infallible;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_types::llm::{LlmProfile, LlmRequest};

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, NamePath};
use crate::router::ApiState;
use crate::sse::sse_response;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .merge(crate::api::resource::scripts::routes())
        .merge(crate::api::resource::tools::routes())
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
}

// ── LLM generation ────────────────────────────────────────────────

async fn handle_generate(
    State(state): State<ApiState>,
    Json(request): Json<LlmRequest>,
) -> impl IntoResponse {
    match wf_api::llm::llm::generate(&state.ctx, &request).await {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_generate_batch(
    State(state): State<ApiState>,
    Json(requests): Json<Vec<LlmRequest>>,
) -> impl IntoResponse {
    match wf_api::llm::llm::generate_batch(&state.ctx, &requests).await {
        Ok(results) => ok(results).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_generate_stream(
    State(state): State<ApiState>,
    Json(request): Json<LlmRequest>,
) -> Response {
    let stream = match wf_api::llm::llm::generate_stream(&state.ctx, &request).await {
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

async fn handle_count_tokens(
    State(state): State<ApiState>,
    Json(request): Json<LlmRequest>,
) -> impl IntoResponse {
    match wf_api::llm::llm::count_tokens(&state.ctx, &request).await {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

// ── LLM profiles ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListProfilesQuery {
    id: Option<String>,
    name: Option<String>,
    provider: Option<String>,
    model: Option<String>,
}

async fn handle_list_profiles(
    State(state): State<ApiState>,
    Query(query): Query<ListProfilesQuery>,
) -> impl IntoResponse {
    let filter = wf_api::LlmProfileFilter {
        id: query.id,
        name: query.name,
        provider: query
            .provider
            .as_deref()
            .and_then(|p| serde_json::from_value(serde_json::json!(p)).ok()),
        model: query.model,
    };
    match wf_api::llm::llm_profile::query(&state.ctx, &filter).await {
        Ok(profiles) => ok(profiles).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_create_profile(
    State(state): State<ApiState>,
    Json(profile): Json<wf_types::llm::profile::LlmProfile>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::create(&state.ctx, &profile).await {
        Ok(()) => ok(profile.id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::get(&state.ctx, &path.id).await {
        Ok(profile) => ok(profile).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_update_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut profile): Json<wf_types::llm::profile::LlmProfile>,
) -> impl IntoResponse {
    profile.id = path.id;
    match wf_api::llm::llm_profile::update(&state.ctx, &profile).await {
        Ok(()) => ok(profile.id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_delete_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::delete(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_set_default(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::set_default(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_default(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::llm_profile::get_default(&state.ctx).await {
        Ok(profile) => ok(profile).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_export_profile(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::export_json(&state.ctx, &path.id).await {
        Ok(json) => ok(json).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ImportProfileBody {
    json: String,
}

async fn handle_import_profile(
    State(state): State<ApiState>,
    Json(body): Json<ImportProfileBody>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::import_json(&state.ctx, &body.json).await {
        Ok(id) => ok(id).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_export_all_profiles(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::llm_profile::export_all_json(&state.ctx).await {
        Ok(json) => ok(json).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_import_all_profiles(
    State(state): State<ApiState>,
    Json(body): Json<ImportProfileBody>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::import_all_json(&state.ctx, &body.json).await {
        Ok(ids) => ok(ids).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_list_templates(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::llm::llm_profile::list_templates(&state.ctx).await {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_add_template(
    State(state): State<ApiState>,
    Json(template): Json<wf_api::LlmProfileTemplate>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::add_template(&state.ctx, template).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct TemplateNameQuery {
    name: String,
}

async fn handle_remove_template(
    State(state): State<ApiState>,
    Query(query): Query<TemplateNameQuery>,
) -> impl IntoResponse {
    match wf_api::llm::llm_profile::remove_template(&state.ctx, &query.name).await {
        Ok(removed) => ok(removed).into_response(),
        Err(e) => error_response(e),
    }
}

/// Single profile template by name (built-in or custom).
async fn handle_get_template_by_name(
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
async fn handle_validate_profile(
    State(state): State<ApiState>,
    Json(profile): Json<LlmProfile>,
) -> impl IntoResponse {
    let (valid, errors) = wf_api::llm::llm_profile::validate(&state.ctx, &profile);
    ok(serde_json::json!({ "valid": valid, "errors": errors })).into_response()
}

#[derive(Deserialize)]
struct CreateFromTemplateBody {
    template_name: String,
    overrides: Value,
}

async fn handle_create_from_template(
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
    async fn llm_profile_and_registry_endpoints_are_reachable() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/llm/profiles",
            "/api/v1/llm/profiles/default",
            "/api/v1/llm/profile-templates",
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
                            "provider": "mock"
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

//! Script registry surface: execute / validate / CRUD / enable / disable.
//! Handlers are thin transport adapters over the `wf-api::llm::script`
//! surface.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_api::ScriptListOptions;
use wf_api::ScriptStorageMetadata;

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── scripts ──
        .route("/scripts/execute", post(handle_execute_script))
        .route("/scripts/validate", post(handle_validate_script))
        .route(
            "/scripts",
            get(handle_list_scripts).post(handle_save_script),
        )
        .route("/scripts/search", get(handle_search_scripts))
        .route(
            "/scripts/{id}",
            get(handle_get_script)
                .put(handle_update_script)
                .delete(handle_delete_script),
        )
        .route("/scripts/{id}/enable", post(handle_enable_script))
        .route("/scripts/{id}/disable", post(handle_disable_script))
}

// ── scripts ───────────────────────────────────────────────────────

/// Wire shape of `/scripts/execute` and `/scripts/validate`: mirrors
/// `wf_api::ScriptExecuteParams` with deserializable field types.
#[derive(Deserialize)]
pub(crate) struct ScriptExecuteBody {
    name: String,
    language: Option<wf_api::ScriptLanguage>,
    code: Option<String>,
    template: Option<String>,
    #[serde(default)]
    args: std::collections::HashMap<String, Value>,
    sandbox: Option<wf_api::SandboxConfig>,
    working_directory: Option<String>,
    environment: Option<std::collections::HashMap<String, String>>,
    timeout_ms: Option<u64>,
    max_output_bytes: Option<u64>,
    output_spill_dir: Option<String>,
}

impl ScriptExecuteBody {
    fn into_params(self) -> wf_api::ScriptExecuteParams {
        wf_api::ScriptExecuteParams {
            name: self.name,
            language: self.language,
            code: self.code,
            template: self.template,
            args: self.args,
            sandbox: self.sandbox,
            working_directory: self.working_directory,
            environment: self.environment,
            timeout_ms: self.timeout_ms,
            max_output_bytes: self.max_output_bytes,
            output_spill_dir: self.output_spill_dir,
        }
    }
}

#[utoipa::path(
    post,
    path = "/scripts/execute",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execute_script(
    State(state): State<ApiState>,
    Json(body): Json<ScriptExecuteBody>,
) -> impl IntoResponse {
    match wf_api::llm::script::execute(&state.ctx, &body.into_params()).await {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/scripts/validate",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_validate_script(
    State(state): State<ApiState>,
    Json(body): Json<ScriptExecuteBody>,
) -> impl IntoResponse {
    match wf_api::llm::script::validate(&state.ctx, &body.into_params()).await {
        Ok(result) => ok(result).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct ListScriptsQuery {
    #[serde(flatten)]
    page: ListQuery,
    language: Option<String>,
}

#[utoipa::path(
    get,
    path = "/scripts",
    tag = "llm",
    params(("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset"), ("language" = Option<String>, Query, description = "language")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_scripts(
    State(state): State<ApiState>,
    Query(query): Query<ListScriptsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    if let Some(language) = query.language {
        match wf_api::llm::script::list_scripts_by_language(&state.ctx.storage, &language).await {
            Ok(scripts) => {
                let window = scripts
                    .into_iter()
                    .skip(offset as usize)
                    .take(fetch_size(limit) as usize)
                    .collect();
                ok_page(window, limit, offset).into_response()
            }
            Err(e) => error_response(e),
        }
    } else {
        let options = ScriptListOptions {
            offset: Some(offset),
            limit: Some(fetch_size(limit)),
            language_filter: None,
        };
        match wf_api::llm::script::list_scripts(&state.ctx.storage, Some(options)).await {
            Ok(scripts) => ok_page(scripts, limit, offset).into_response(),
            Err(e) => error_response(e),
        }
    }
}

#[utoipa::path(
    post,
    path = "/scripts",
    tag = "llm",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_save_script(
    State(state): State<ApiState>,
    Json(script): Json<ScriptStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::llm::script::save_script(&state.ctx.storage, &script).await {
        Ok(()) => ok(script.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct SearchScriptsQuery {
    q: String,
    #[serde(flatten)]
    page: ListQuery,
}

#[utoipa::path(
    get,
    path = "/scripts/search",
    tag = "llm",
    params(("q" = String, Query, description = "q"), ("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_search_scripts(
    State(state): State<ApiState>,
    Query(query): Query<SearchScriptsQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    match wf_api::llm::script::search_scripts(&state.ctx.storage, &query.q).await {
        Ok(scripts) => {
            let window = scripts
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
    get,
    path = "/scripts/{id}",
    tag = "llm",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_script(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::script::get_script(&state.ctx.storage, &path.id).await {
        Ok(script) => ok(script).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    put,
    path = "/scripts/{id}",
    tag = "llm",
    params(("id" = String, Path, description = "id")),
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_update_script(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(mut script): Json<ScriptStorageMetadata>,
) -> impl IntoResponse {
    script.id = wf_api::Id::from(path.id.clone());
    match wf_api::llm::script::save_script(&state.ctx.storage, &script).await {
        Ok(()) => ok(path.id).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct DeleteForceQuery {
    pub(crate) force: Option<bool>,
}

#[utoipa::path(
    delete,
    path = "/scripts/{id}",
    tag = "llm",
    params(("id" = String, Path, description = "id"), ("force" = Option<bool>, Query, description = "force")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_script(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<DeleteForceQuery>,
) -> impl IntoResponse {
    match wf_api::infra::reference::delete_with_reference_check(
        &state.ctx,
        wf_api::ReferenceKind::Script,
        &path.id,
        query.force.unwrap_or(false),
    )
    .await
    {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/scripts/{id}/enable",
    tag = "llm",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_enable_script(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::script::enable_script(&state.ctx.storage, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/scripts/{id}/disable",
    tag = "llm",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_disable_script(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::llm::script::disable_script(&state.ctx.storage, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

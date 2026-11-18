//! Server-owned backend management surface: registry hosting, tunnel
//! supervision and scheduled backups. All routes require normal auth and
//! return 503 while the backend domain is disabled, so the direct-link
//! chain keeps zero behavior change.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::backends::types::{BackendEntry, BackendEntryView, BackupRecordView};
use crate::envelope::{error_response, ok, service_unavailable};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/system/backends", get(handle_list).post(handle_create))
        .route("/system/backends/import", post(handle_import))
        .route("/system/backups", get(handle_list_all_backups))
        .route(
            "/system/backends/{name}",
            get(handle_get).put(handle_update).delete(handle_remove),
        )
        .route(
            "/system/backends/{name}/tunnel/start",
            post(handle_tunnel_start),
        )
        .route(
            "/system/backends/{name}/tunnel/stop",
            post(handle_tunnel_stop),
        )
        .route(
            "/system/backends/{name}/backups",
            get(handle_list_backups).post(handle_trigger_backup),
        )
}

fn disabled() -> axum::response::Response {
    service_unavailable("backend management is disabled; enable it in backends.toml")
        .into_response()
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct BackendListView {
    backends: Vec<BackendEntryView>,
    source: String,
    backup_round_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[allow(dead_code)]
pub(crate) struct BackendUpsertBody {
    #[serde(flatten)]
    entry: BackendEntry,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub(crate) struct BackendImportBody {
    #[serde(default)]
    hosts: std::collections::BTreeMap<String, BackendEntry>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct BackendImportView {
    imported: Vec<String>,
    skipped: Vec<ImportSkippedView>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ImportSkippedView {
    name: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct TunnelStartView {
    name: String,
    pid: u32,
    argv: Vec<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct TunnelStopView {
    name: String,
    stopped: bool,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub(crate) struct BackupTriggerBody {
    execution: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct BackupTriggerView {
    id: String,
    backend: String,
    execution: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct BackupListView {
    backups: Vec<BackupRecordView>,
}

#[utoipa::path(get, path = "/api/v1/system/backends", operation_id = "list_system_backends", tag = "system", responses((status = 200, description = "Backend registry with live status", body = crate::envelope::ApiEnvelope<BackendListView>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_list(State(state): State<ApiState>) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    match backends.list_with_status().await {
        Ok(rows) => ok(BackendListView {
            backends: rows,
            source: "server".to_string(),
            backup_round_error: backends.backup_round_error(),
        })
        .into_response(),
        Err(e) => error_response(wf_api::ApiError::execution(e)).into_response(),
    }
}

#[utoipa::path(post, path = "/api/v1/system/backends", operation_id = "create_system_backend", tag = "system", request_body = BackendUpsertBody, responses((status = 200, description = "Backend created", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_create(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    let (name, entry) = match parse_upsert_body(&body) {
        Ok(parsed) => parsed,
        Err(e) => {
            return error_response(wf_api::ApiError::Validation(e)).into_response();
        }
    };
    match backends.create(&name, entry).await {
        Ok(()) => ok(serde_json::json!({"name": name})).into_response(),
        Err(e) if e.contains("already exists") => error_response(wf_api::ApiError::AlreadyExists {
            entity_type: "backend".to_string(),
            id: name,
        })
        .into_response(),
        Err(e) => error_response(wf_api::ApiError::Validation(e)).into_response(),
    }
}

#[utoipa::path(get, path = "/api/v1/system/backends/{name}", operation_id = "get_system_backend", tag = "system", params(crate::extract::NamePath), responses((status = 200, description = "Backend with live status", body = crate::envelope::ApiEnvelope<BackendEntryView>), (status = 404, description = "Unknown backend", body = crate::envelope::ErrorResponse), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_get(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    match backends.get_with_status(&name).await {
        Ok(Some(view)) => ok(view).into_response(),
        Ok(None) => error_response(wf_api::ApiError::NotFound {
            entity_type: "backend".to_string(),
            id: name,
        })
        .into_response(),
        Err(e) => error_response(wf_api::ApiError::execution(e)).into_response(),
    }
}

#[utoipa::path(put, path = "/api/v1/system/backends/{name}", operation_id = "update_system_backend", tag = "system", params(crate::extract::NamePath), request_body = BackendUpsertBody, responses((status = 200, description = "Backend updated", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_update(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    let entry: BackendEntry = match serde_json::from_value(body) {
        Ok(entry) => entry,
        Err(e) => {
            return error_response(wf_api::ApiError::Validation(format!(
                "invalid backend entry: {e}"
            )))
            .into_response();
        }
    };
    match backends.update(&name, entry).await {
        Ok(()) => ok(serde_json::json!({"name": name})).into_response(),
        Err(e) if e.contains("not found") => error_response(wf_api::ApiError::NotFound {
            entity_type: "backend".to_string(),
            id: name,
        })
        .into_response(),
        Err(e) => error_response(wf_api::ApiError::Validation(e)).into_response(),
    }
}

#[utoipa::path(delete, path = "/api/v1/system/backends/{name}", operation_id = "delete_system_backend", tag = "system", params(crate::extract::NamePath), responses((status = 200, description = "Backend removed", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_remove(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    match backends.remove(&name).await {
        Ok(true) => ok(serde_json::json!({"name": name, "removed": true})).into_response(),
        Ok(false) => error_response(wf_api::ApiError::NotFound {
            entity_type: "backend".to_string(),
            id: name,
        })
        .into_response(),
        Err(e) => error_response(wf_api::ApiError::execution(e)).into_response(),
    }
}

#[utoipa::path(post, path = "/api/v1/system/backends/import", operation_id = "import_system_backends", tag = "system", request_body = BackendImportBody, responses((status = 200, description = "Import report", body = crate::envelope::ApiEnvelope<BackendImportView>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_import(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<BackendImportBody>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    match backends.import_cli(body.hosts).await {
        Ok(report) => ok(BackendImportView {
            imported: report.imported,
            skipped: report
                .skipped
                .into_iter()
                .map(|row| ImportSkippedView {
                    name: row.name,
                    reason: row.reason,
                })
                .collect(),
        })
        .into_response(),
        Err(e) => error_response(wf_api::ApiError::execution(e)).into_response(),
    }
}

#[utoipa::path(post, path = "/api/v1/system/backends/{name}/tunnel/start", operation_id = "start_system_backend_tunnel", tag = "system", params(crate::extract::NamePath), responses((status = 200, description = "Tunnel started", body = crate::envelope::ApiEnvelope<TunnelStartView>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_tunnel_start(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    match backends.start_tunnel(&name).await {
        Ok((pid, argv)) => ok(TunnelStartView { name, pid, argv }).into_response(),
        Err(e) if e.contains("not found") => error_response(wf_api::ApiError::NotFound {
            entity_type: "backend".to_string(),
            id: name,
        })
        .into_response(),
        Err(e) if e.contains("already running") || e.contains("already in use") => {
            error_response(wf_api::ApiError::Conflict(e)).into_response()
        }
        Err(e) => error_response(wf_api::ApiError::Validation(e)).into_response(),
    }
}

#[utoipa::path(post, path = "/api/v1/system/backends/{name}/tunnel/stop", operation_id = "stop_system_backend_tunnel", tag = "system", params(crate::extract::NamePath), responses((status = 200, description = "Tunnel stopped", body = crate::envelope::ApiEnvelope<TunnelStopView>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_tunnel_stop(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    match backends.stop_tunnel(&name).await {
        Ok(stopped) => ok(TunnelStopView { name, stopped }).into_response(),
        Err(e) if e.contains("not found") => error_response(wf_api::ApiError::NotFound {
            entity_type: "backend".to_string(),
            id: name,
        })
        .into_response(),
        Err(e) => error_response(wf_api::ApiError::execution(e)).into_response(),
    }
}

#[utoipa::path(get, path = "/api/v1/system/backends/{name}/backups", operation_id = "list_system_backend_backups", tag = "system", params(crate::extract::NamePath), responses((status = 200, description = "Backup index for one backend", body = crate::envelope::ApiEnvelope<BackupListView>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_list_backups(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    match backends.list_backups(Some(&name)).await {
        Ok(backups) => ok(BackupListView { backups }).into_response(),
        Err(e) => error_response(wf_api::ApiError::execution(e)).into_response(),
    }
}

#[utoipa::path(get, path = "/api/v1/system/backups", operation_id = "list_system_backups", tag = "system", responses((status = 200, description = "Backup index across backends", body = crate::envelope::ApiEnvelope<BackupListView>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_list_all_backups(State(state): State<ApiState>) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    match backends.list_backups(None).await {
        Ok(backups) => ok(BackupListView { backups }).into_response(),
        Err(e) => error_response(wf_api::ApiError::execution(e)).into_response(),
    }
}

#[utoipa::path(post, path = "/api/v1/system/backends/{name}/backups", operation_id = "trigger_system_backend_backup", tag = "system", params(crate::extract::NamePath), request_body = BackupTriggerBody, responses((status = 200, description = "Backup triggered", body = crate::envelope::ApiEnvelope<BackupTriggerView>), (status = 503, description = "Backend management disabled", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_trigger_backup(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    axum::Json(body): axum::Json<BackupTriggerBody>,
) -> impl IntoResponse {
    let Some(backends) = state.backends.clone() else {
        return disabled();
    };
    if !backends.is_enabled() {
        return disabled();
    }
    if body.execution.trim().is_empty() {
        return error_response(wf_api::ApiError::Validation(
            "execution must not be empty".to_string(),
        ))
        .into_response();
    }
    match backends.trigger_backup(&name, &body.execution).await {
        Ok(id) => ok(BackupTriggerView {
            id,
            backend: name,
            execution: body.execution,
        })
        .into_response(),
        Err(e) if e.contains("not found") => error_response(wf_api::ApiError::NotFound {
            entity_type: "backend".to_string(),
            id: name,
        })
        .into_response(),
        Err(e) => error_response(wf_api::ApiError::execution(e)).into_response(),
    }
}

fn parse_upsert_body(body: &serde_json::Value) -> Result<(String, BackendEntry), String> {
    let name = body
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "backend name must not be empty".to_string())?
        .to_string();
    let entry_value = body.get("entry").cloned().unwrap_or_else(|| {
        let mut without_name = body.clone();
        if let Some(object) = without_name.as_object_mut() {
            object.remove("name");
        }
        without_name
    });
    let entry: BackendEntry =
        serde_json::from_value(entry_value).map_err(|e| format!("invalid backend entry: {e}"))?;
    Ok((name, entry))
}

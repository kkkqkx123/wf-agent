// Server identity, pairing and session connect surface.
// Mirrors the reference lifecycle endpoints without adding new storage:
// info reports version and capabilities, pair issues a one-time code over
// an authenticated call, connect consumes the code and issues a session.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Serialize;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/server/info", get(handle_server_info))
        .route("/server/status", get(handle_server_status))
        .route("/server/pair", post(handle_server_pair))
        .route("/server/connect", get(handle_server_connect_query))
        .route("/server/connect/{code}", get(handle_server_connect))
        .route(
            "/server/config",
            get(handle_server_config).put(handle_server_config_update),
        )
        .route("/server/shells", get(handle_server_shells))
        .route(
            "/server/projects",
            get(handle_server_projects).post(handle_server_projects_refresh),
        )
        .route("/server/migration-status", get(handle_migration_status))
        .route("/system/debug/heap", get(handle_debug_heap))
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ServerInfoView {
    version: String,
    pid: u32,
    hostname: Option<String>,
    urls: Vec<String>,
    tmp: String,
    capabilities: ServerCapabilities,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ServerCapabilities {
    #[serde(rename = "persistentPty")]
    persistent_pty: bool,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ServerStatusView {
    stopped: bool,
    version: String,
    pid: u32,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct PairView {
    code: String,
    #[serde(rename = "expiresIn")]
    expires_in: i64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ConnectView {
    token: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ServerConfigView {
    entries: Vec<ConfigEntry>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ConfigEntry {
    key: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ShellsView {
    shells: Vec<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ProjectsView {
    projects: Vec<ProjectEntry>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ProjectEntry {
    #[serde(rename = "projectId")]
    project_id: String,
    directory: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct MigrationStatusView {
    migrated: bool,
    version: u32,
}



fn request_urls(headers: &HeaderMap) -> Vec<String> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("127.0.0.1:3000");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("http");
    vec![format!("{scheme}://{host}")]
}

fn hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.is_empty())
}

#[utoipa::path(
    get,
    path = "/api/v1/server/info",
    tag = "system",
    responses((status = 200, description = "Server identity", body = crate::envelope::ApiEnvelope<ServerInfoView>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_server_info(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let _ = state;
    ok(ServerInfoView {
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        hostname: hostname(),
        urls: request_urls(&headers),
        tmp: std::env::temp_dir().to_string_lossy().into_owned(),
        capabilities: ServerCapabilities {
            persistent_pty: true,
        },
    })
    .into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/server/status",
    tag = "system",
    responses((status = 200, description = "Server stop state", body = crate::envelope::ApiEnvelope<ServerStatusView>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_server_status() -> impl IntoResponse {
    ok(ServerStatusView {
        // The process has no detached stop latch; a running handler is
        // definitionally not stopped. The flag exists so shutdown sequencing
        // stays observable when an embedder adds one later.
        stopped: false,
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
    })
    .into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/server/pair",
    tag = "system",
    responses((status = 200, description = "One-time pairing code", body = crate::envelope::ApiEnvelope<PairView>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_server_pair() -> impl IntoResponse {
    let (code, expires_in) = crate::pairing::issue();
    ok(PairView { code, expires_in }).into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/server/connect",
    tag = "system",
    params(crate::extract::CodeQuery),
    responses(
        (status = 200, description = "Session token", body = crate::envelope::ApiEnvelope<ConnectView>),
        (status = 403, description = "Expired or consumed code", body = crate::envelope::ErrorResponse),
    ),
    security(())
)]
pub(crate) async fn handle_server_connect_query(
    State(state): State<ApiState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<crate::extract::CodeQuery>,
) -> Response {
    match query.code {
        Some(code) => connect_with_code(&state, &headers, &code).await,
        None => error_response(wf_api::ApiError::Validation(
            "pairing code is required".to_string(),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/server/connect/{code}",
    tag = "system",
    params(crate::extract::CodePath),
    responses(
        (status = 200, description = "Session token", body = crate::envelope::ApiEnvelope<ConnectView>),
        (status = 403, description = "Expired or consumed code", body = crate::envelope::ErrorResponse),
    ),
    security(())
)]
pub(crate) async fn handle_server_connect(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(path): Path<crate::extract::CodePath>,
) -> Response {
    connect_with_code(&state, &headers, &path.code).await
}

async fn connect_with_code(state: &ApiState, headers: &HeaderMap, code: &str) -> Response {
    if !crate::pairing::consume(code) {
        return crate::envelope::ticket_invalid("Pairing link expired or already used.");
    }
    let Some(password) = state
        .config
        .auth
        .password
        .clone()
        .filter(|value| !value.is_empty())
    else {
        return error_response(wf_api::ApiError::Validation(
            "server password is not configured".to_string(),
        ));
    };
    let token = crate::auth_session::issue_session(&password, wf_common::epoch_ms() as i64);
    let browser = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/html"));
    if !browser {
        return ok(ConnectView { token }).into_response();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let cookie_name = crate::auth_session::session_cookie_name(host);
    let mut response = axum::http::Response::builder()
        .status(axum::http::StatusCode::FOUND)
        .header(header::LOCATION, "/")
        .body(axum::body::Body::from("paired"))
        .unwrap_or_else(|_| crate::envelope::unauthorized("pairing failed"));
    if let Ok(value) = format!(
        "{cookie_name}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        crate::auth_session::SESSION_TTL_SECS
    )
    .parse::<axum::http::HeaderValue>()
    {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

#[utoipa::path(
    get,
    path = "/api/v1/server/config",
    tag = "system",
    responses((status = 200, description = "Effective server config entries", body = crate::envelope::ApiEnvelope<ServerConfigView>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_server_config(State(state): State<ApiState>) -> impl IntoResponse {
    let _ = state;
    ok(ServerConfigView {
        entries: Vec::new(),
    })
    .into_response()
}

#[utoipa::path(
    put,
    path = "/api/v1/server/config",
    tag = "system",
    responses(
        (status = 503, description = "Config updates unavailable", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_server_config_update() -> impl IntoResponse {
    crate::envelope::service_unavailable("server config updates are not supported on this platform")
}

#[utoipa::path(
    get,
    path = "/api/v1/server/shells",
    tag = "system",
    responses((status = 200, description = "Discoverable shells", body = crate::envelope::ApiEnvelope<ShellsView>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_server_shells() -> impl IntoResponse {
    let mut shells = Vec::new();
    for candidate in ["/bin/bash", "/bin/zsh", "/bin/sh"] {
        if std::path::Path::new(candidate).exists() {
            shells.push(candidate.to_string());
        }
    }
    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.is_empty() && !shells.contains(&shell) {
            shells.push(shell);
        }
    }
    ok(ShellsView { shells }).into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/server/projects",
    tag = "system",
    responses((status = 200, description = "Cached projects", body = crate::envelope::ApiEnvelope<ProjectsView>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_server_projects(State(state): State<ApiState>) -> impl IntoResponse {
    ok(ProjectsView {
        projects: cached_projects(&state),
    })
    .into_response()
}

fn cached_projects(state: &ApiState) -> Vec<ProjectEntry> {
    state
        .dirs
        .cached_keys()
        .into_iter()
        .map(|directory| {
            let project = crate::directory::discover_project(std::path::Path::new(&directory));
            ProjectEntry {
                project_id: crate::directory::project_id_for(&project),
                directory,
            }
        })
        .collect()
}

#[utoipa::path(
    post,
    path = "/api/v1/server/projects",
    tag = "system",
    responses((status = 200, description = "Revalidated projects", body = crate::envelope::ApiEnvelope<ProjectsView>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_server_projects_refresh(State(state): State<ApiState>) -> impl IntoResponse {
    // No persisted project table: refresh re-reads the validated directory
    // cache so equivalent spellings collapse to the same stable id.
    ok(ProjectsView {
        projects: cached_projects(&state),
    })
    .into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/server/migration-status",
    tag = "system",
    responses((status = 200, description = "Migration status", body = crate::envelope::ApiEnvelope<MigrationStatusView>)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_migration_status() -> impl IntoResponse {
    // No pending migrations: the empty state keeps versioned extensibility
    // without claiming work that never ran.
    ok(MigrationStatusView {
        migrated: false,
        version: 0,
    })
    .into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/system/debug/heap",
    tag = "system",
    responses((status = 503, description = "Heap snapshot unavailable", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_debug_heap() -> impl IntoResponse {
    crate::envelope::service_unavailable("heap snapshots are not supported on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body as AxBody;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_router() -> Router {
        crate::router::api_router(test_ctx())
    }

    fn test_ctx() -> std::sync::Arc<wf_api::ApiContext> {
        std::sync::Arc::new(wf_api::ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    #[tokio::test]
    async fn server_info_is_public() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/server/info")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn pair_then_connect_issues_token() {
        let pair = test_router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/server/pair")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pair.status(), axum::http::StatusCode::OK);
    }
}

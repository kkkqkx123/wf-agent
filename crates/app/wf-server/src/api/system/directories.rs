//! System surface for multi-directory serving: inspect the validated
//! directory cache and drop single entries so the next request revalidates
//! them from disk. Read-only by nature; invalidation never touches
//! in-flight requests.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/system/directories", get(handle_directories))
        .route(
            "/system/directories/resolve",
            get(handle_directories_resolve),
        )
        .route(
            "/system/directories/invalidate",
            post(handle_directories_invalidate),
        )
        .route("/system/debug/location", get(handle_debug_location))
}

/// One cached workspace: the directory the client named plus the owning
/// project root and stable project id derived from it.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct DirectoryProjectView {
    directory: String,
    project: String,
    project_id: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct DirectoriesView {
    allowed_roots: Vec<String>,
    allow_any_explicit: bool,
    cached_entries: usize,
    cached_keys: Vec<String>,
    /// Cached scopes with project identity, so workspace pickers can
    /// show "directory in project" without a second round trip.
    projects: Vec<DirectoryProjectView>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub(crate) struct DirectoriesInvalidateBody {
    directory: String,
}

#[utoipa::path(get, path = "/api/v1/system/directories", operation_id = "get_system_directories", tag = "system", responses((status = 200, description = "Directory serving state", body = crate::envelope::ApiEnvelope<DirectoriesView>)), security(("api_key" = [])))]
pub(crate) async fn handle_directories(State(state): State<ApiState>) -> impl IntoResponse {
    ok(DirectoriesView {
        allowed_roots: state
            .dirs
            .config()
            .allowed_roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect(),
        allow_any_explicit: state.dirs.config().allowed_roots.is_empty(),
        cached_entries: state.dirs.entry_count(),
        cached_keys: state.dirs.cached_keys(),
        projects: state
            .dirs
            .cached_scopes()
            .into_iter()
            .map(|scope| DirectoryProjectView {
                directory: scope.directory.to_string_lossy().into_owned(),
                project: scope.project.to_string_lossy().into_owned(),
                project_id: scope.project_id,
            })
            .collect(),
    })
    .into_response()
}

/// Content fingerprint of the anchored project for cross-machine affinity:
/// the version-control origin plus commit first, then a committed marker id
/// for checkouts without version control. Empty when the project carries no
/// cross-machine identity.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct BackendFingerprintView {
    git_origin: Option<String>,
    git_head: Option<String>,
    marker_id: Option<String>,
}

/// The scope this request is anchored on, for backend affinity checks.
/// Returns hashes and fingerprints only, never raw server paths beyond
/// the directory the client itself named.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct DirectoriesResolveView {
    project_id: String,
    fingerprint: BackendFingerprintView,
    directory_exists: bool,
}

#[utoipa::path(get, path = "/api/v1/system/directories/resolve", operation_id = "get_system_directories_resolve", tag = "system", responses((status = 200, description = "Anchored scope identity", body = crate::envelope::ApiEnvelope<DirectoriesResolveView>)), security(("api_key" = [])))]
pub(crate) async fn handle_directories_resolve(
    axum::Extension(scope): axum::Extension<Option<crate::directory::DirectoryScope>>,
) -> impl IntoResponse {
    let Some(scope) = scope else {
        return error_response(wf_api::ApiError::Validation(
            "directory is required: send x-wf-directory or ?directory".to_string(),
        ))
        .into_response();
    };
    let fingerprint = wf_common::workspace::project_fingerprint(&scope.project).await;
    let directory_exists = tokio::fs::metadata(&scope.directory)
        .await
        .is_ok_and(|meta| meta.is_dir());
    ok(DirectoriesResolveView {
        project_id: scope.project_id,
        fingerprint: BackendFingerprintView {
            git_origin: fingerprint.git_origin,
            git_head: fingerprint.git_head,
            marker_id: fingerprint.marker_id,
        },
        directory_exists,
    })
    .into_response()
}

#[utoipa::path(get, path = "/api/v1/system/debug/location", operation_id = "get_system_debug_location", tag = "system", responses((status = 200, description = "Cached location keys", body = crate::envelope::ApiEnvelope<Vec<String>>)), security(("api_key" = [])))]
pub(crate) async fn handle_debug_location(State(state): State<ApiState>) -> impl IntoResponse {
    ok(state.dirs.cached_keys()).into_response()
}

#[utoipa::path(post, path = "/api/v1/system/directories/invalidate", operation_id = "post_system_directories_invalidate", tag = "system", request_body = DirectoriesInvalidateBody, responses((status = 200, description = "Cache entry dropped", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "No such cached entry", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_directories_invalidate(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<DirectoriesInvalidateBody>,
) -> impl IntoResponse {
    if state.dirs.invalidate(body.directory.trim()) {
        ok(serde_json::json!({"invalidated": true})).into_response()
    } else {
        error_response(wf_api::ApiError::NotFound {
            entity_type: "directory".to_string(),
            id: body.directory,
        })
    }
}

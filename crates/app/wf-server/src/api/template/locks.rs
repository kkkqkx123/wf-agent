//! Node and trigger template editing locks, sharing the definition lock
//! store with workflow and agent locks via a kind dimension.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::api::workflow::locks::{publish_lock_event, AcquireLockBody, LockView, OwnerBody};
use crate::envelope::{err, ok, ApiError};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/templates/node/{id}/lock", get(handle_get_node_lock))
        .route(
            "/templates/node/{id}/lock/acquire",
            post(handle_acquire_node_lock),
        )
        .route(
            "/templates/node/{id}/lock/heartbeat",
            post(handle_heartbeat_node_lock),
        )
        .route(
            "/templates/node/{id}/lock/release",
            post(handle_release_node_lock),
        )
        .route("/templates/trigger/{id}/lock", get(handle_get_trigger_lock))
        .route(
            "/templates/trigger/{id}/lock/acquire",
            post(handle_acquire_trigger_lock),
        )
        .route(
            "/templates/trigger/{id}/lock/heartbeat",
            post(handle_heartbeat_trigger_lock),
        )
        .route(
            "/templates/trigger/{id}/lock/release",
            post(handle_release_trigger_lock),
        )
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn current(state: &ApiState, kind: &str, id: &str) -> impl IntoResponse {
    ok(state.locks.current(kind, id, now_ms())).into_response()
}

async fn acquire(
    state: &ApiState,
    kind: &str,
    id: &str,
    body: AcquireLockBody,
) -> axum::response::Response {
    if body.owner_id.is_empty() {
        return err(ApiError::validation("owner_id must not be empty")).into_response();
    }
    let before = state
        .locks
        .current(kind, id, now_ms())
        .map(|view| view.owner_id);
    let view = state
        .locks
        .acquire(kind, id, &body.owner_id, &body.owner_name);
    if before.as_deref() != Some(view.owner_id.as_str()) {
        publish_lock_event(
            &state.ctx,
            kind,
            id,
            "acquired",
            &view.owner_id,
            &view.owner_name,
        )
        .await;
    }
    ok(view).into_response()
}

async fn heartbeat(
    state: &ApiState,
    kind: &str,
    id: &str,
    body: OwnerBody,
) -> axum::response::Response {
    let before = state.locks.current(kind, id, now_ms());
    match state.locks.heartbeat(kind, id, &body.owner_id) {
        Some(view) => ok(view).into_response(),
        None => {
            if let Some(prev) = before {
                publish_lock_event(
                    &state.ctx,
                    kind,
                    id,
                    "expired",
                    &prev.owner_id,
                    &prev.owner_name,
                )
                .await;
            }
            err(ApiError::validation("no active lease to renew")).into_response()
        }
    }
}

async fn release(
    state: &ApiState,
    kind: &str,
    id: &str,
    body: OwnerBody,
) -> axum::response::Response {
    let before = state.locks.current(kind, id, now_ms());
    let after: Option<LockView> = state.locks.release(kind, id, &body.owner_id);
    if let Some(prev) = before {
        let changed = after
            .as_ref()
            .is_none_or(|view| view.owner_id != prev.owner_id);
        if changed {
            publish_lock_event(
                &state.ctx,
                kind,
                id,
                "released",
                &prev.owner_id,
                &prev.owner_name,
            )
            .await;
        }
    }
    ok(after).into_response()
}

// ── node template locks ──────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/templates/node/{id}/lock",
    operation_id = "get_templates_node_id_lock",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Current lease holder, null when unlocked", body = crate::envelope::ApiEnvelope<Option<crate::api::workflow::locks::LockView>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_node_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    current(&state, "node", &path.id).await
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/node/{id}/lock/acquire",
    operation_id = "post_templates_node_id_lock_acquire",
    tag = "template",
    params(IdPath),
    request_body = AcquireLockBody,
    responses((status = 200, description = "Active holder after the claim attempt", body = crate::envelope::ApiEnvelope<crate::api::workflow::locks::LockView>), (status = 400, description = "Missing owner id", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_acquire_node_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<AcquireLockBody>,
) -> impl IntoResponse {
    acquire(&state, "node", &path.id, body).await
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/node/{id}/lock/heartbeat",
    operation_id = "post_templates_node_id_lock_heartbeat",
    tag = "template",
    params(IdPath),
    request_body = OwnerBody,
    responses((status = 200, description = "Active holder after the renewal attempt", body = crate::envelope::ApiEnvelope<crate::api::workflow::locks::LockView>), (status = 400, description = "No active lease to renew", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_heartbeat_node_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<OwnerBody>,
) -> impl IntoResponse {
    heartbeat(&state, "node", &path.id, body).await
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/node/{id}/lock/release",
    operation_id = "post_templates_node_id_lock_release",
    tag = "template",
    params(IdPath),
    request_body = OwnerBody,
    responses((status = 200, description = "Remaining holder, null after a successful release", body = crate::envelope::ApiEnvelope<Option<crate::api::workflow::locks::LockView>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_release_node_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<OwnerBody>,
) -> impl IntoResponse {
    release(&state, "node", &path.id, body).await
}

// ── trigger template locks ───────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/templates/trigger/{id}/lock",
    operation_id = "get_templates_trigger_id_lock",
    tag = "template",
    params(IdPath),
    responses((status = 200, description = "Current lease holder, null when unlocked", body = crate::envelope::ApiEnvelope<Option<crate::api::workflow::locks::LockView>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_trigger_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    current(&state, "trigger", &path.id).await
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger/{id}/lock/acquire",
    operation_id = "post_templates_trigger_id_lock_acquire",
    tag = "template",
    params(IdPath),
    request_body = AcquireLockBody,
    responses((status = 200, description = "Active holder after the claim attempt", body = crate::envelope::ApiEnvelope<crate::api::workflow::locks::LockView>), (status = 400, description = "Missing owner id", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_acquire_trigger_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<AcquireLockBody>,
) -> impl IntoResponse {
    acquire(&state, "trigger", &path.id, body).await
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger/{id}/lock/heartbeat",
    operation_id = "post_templates_trigger_id_lock_heartbeat",
    tag = "template",
    params(IdPath),
    request_body = OwnerBody,
    responses((status = 200, description = "Active holder after the renewal attempt", body = crate::envelope::ApiEnvelope<crate::api::workflow::locks::LockView>), (status = 400, description = "No active lease to renew", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_heartbeat_trigger_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<OwnerBody>,
) -> impl IntoResponse {
    heartbeat(&state, "trigger", &path.id, body).await
}

#[utoipa::path(
    post,
    path = "/api/v1/templates/trigger/{id}/lock/release",
    operation_id = "post_templates_trigger_id_lock_release",
    tag = "template",
    params(IdPath),
    request_body = OwnerBody,
    responses((status = 200, description = "Remaining holder, null after a successful release", body = crate::envelope::ApiEnvelope<Option<crate::api::workflow::locks::LockView>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_release_trigger_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<OwnerBody>,
) -> impl IntoResponse {
    release(&state, "trigger", &path.id, body).await
}

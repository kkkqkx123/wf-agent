//! Agent definition editing locks, sharing the definition lock store with
//! workflow locks via a kind dimension.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::envelope::{err, ok, ApiError};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/agents/{id}/lock", get(handle_get_lock))
        .route("/agents/{id}/lock/acquire", post(handle_acquire_lock))
        .route("/agents/{id}/lock/heartbeat", post(handle_heartbeat_lock))
        .route("/agents/{id}/lock/release", post(handle_release_lock))
}

use crate::api::workflow::locks::{publish_lock_event, AcquireLockBody, OwnerBody};

#[utoipa::path(
    get,
    path = "/api/v1/agents/{id}/lock",
    operation_id = "get_agents_id_lock",
    tag = "agent",
    params(IdPath),
    responses((status = 200, description = "Current lease holder, null when unlocked", body = crate::envelope::ApiEnvelope<Option<crate::api::workflow::locks::LockView>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    ok(state.locks.current("agent", &path.id, now_ms())).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/agents/{id}/lock/acquire",
    operation_id = "post_agents_id_lock_acquire",
    tag = "agent",
    params(IdPath),
    request_body = AcquireLockBody,
    responses((status = 200, description = "Active holder after the claim attempt", body = crate::envelope::ApiEnvelope<crate::api::workflow::locks::LockView>), (status = 400, description = "Missing owner id", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_acquire_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<AcquireLockBody>,
) -> impl IntoResponse {
    if body.owner_id.is_empty() {
        return err(ApiError::validation("owner_id must not be empty")).into_response();
    }
    let before = state
        .locks
        .current("agent", &path.id, now_ms())
        .map(|view| view.owner_id);
    let view = state
        .locks
        .acquire("agent", &path.id, &body.owner_id, &body.owner_name);
    if before.as_deref() != Some(view.owner_id.as_str()) {
        publish_lock_event(
            &state.ctx,
            "agent",
            &path.id,
            "acquired",
            &view.owner_id,
            &view.owner_name,
        )
        .await;
    }
    ok(view).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/agents/{id}/lock/heartbeat",
    operation_id = "post_agents_id_lock_heartbeat",
    tag = "agent",
    params(IdPath),
    request_body = OwnerBody,
    responses((status = 200, description = "Active holder after the renewal attempt", body = crate::envelope::ApiEnvelope<crate::api::workflow::locks::LockView>), (status = 400, description = "No active lease to renew", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_heartbeat_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<OwnerBody>,
) -> impl IntoResponse {
    let before = state.locks.current("agent", &path.id, now_ms());
    match state.locks.heartbeat("agent", &path.id, &body.owner_id) {
        Some(view) => ok(view).into_response(),
        None => {
            if let Some(prev) = before {
                publish_lock_event(
                    &state.ctx,
                    "agent",
                    &path.id,
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

#[utoipa::path(
    post,
    path = "/api/v1/agents/{id}/lock/release",
    operation_id = "post_agents_id_lock_release",
    tag = "agent",
    params(IdPath),
    request_body = OwnerBody,
    responses((status = 200, description = "Remaining holder, null after a successful release", body = crate::envelope::ApiEnvelope<Option<crate::api::workflow::locks::LockView>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_release_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<OwnerBody>,
) -> impl IntoResponse {
    let before = state.locks.current("agent", &path.id, now_ms());
    let after = state.locks.release("agent", &path.id, &body.owner_id);
    if let Some(prev) = before {
        let changed = after
            .as_ref()
            .is_none_or(|view| view.owner_id != prev.owner_id);
        if changed {
            publish_lock_event(
                &state.ctx,
                "agent",
                &path.id,
                "released",
                &prev.owner_id,
                &prev.owner_name,
            )
            .await;
        }
    }
    ok(after).into_response()
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

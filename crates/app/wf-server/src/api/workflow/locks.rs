//! Single-writer editing lease per workflow, held in process memory.
//!
//! The web editor claims a lease before entering edit mode, renews it with a
//! heartbeat and releases it on exit. Lease state is intentionally not
//! persisted: a restart simply frees every lease, which is safe because a
//! stale client re-acquires on its next poll.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;

use crate::envelope::{err, ok, ApiError};
use crate::extract::IdPath;
use crate::router::ApiState;

/// Lease duration; comfortably longer than the client heartbeat interval so
/// one dropped beat does not free the lease.
const LEASE_MS: u64 = 90_000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone)]
pub(crate) struct WorkflowLocks {
    leases: Arc<Mutex<HashMap<String, Lease>>>,
}

impl Default for WorkflowLocks {
    fn default() -> Self {
        Self {
            leases: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

struct Lease {
    owner_id: String,
    owner_name: String,
    expires_at: u64,
}

impl Lease {
    fn active(&self, now: u64) -> bool {
        self.expires_at > now
    }

    fn view(&self) -> LockView {
        LockView {
            owner_id: self.owner_id.clone(),
            owner_name: self.owner_name.clone(),
            expires_at: self.expires_at,
        }
    }
}

impl WorkflowLocks {
    fn current(&self, workflow_id: &str, now: u64) -> Option<LockView> {
        let leases = self.leases.lock().expect("lock map mutex");
        leases
            .get(workflow_id)
            .filter(|lease| lease.active(now))
            .map(Lease::view)
    }

    /// Claim the lease when free (or renew own claim); an active claim by a
    /// different owner wins and is reported back untouched.
    fn acquire(&self, workflow_id: &str, owner_id: &str, owner_name: &str) -> LockView {
        let now = now_ms();
        let mut leases = self.leases.lock().expect("lock map mutex");
        let entry = leases.entry(workflow_id.to_string()).or_insert(Lease {
            owner_id: owner_id.to_string(),
            owner_name: owner_name.to_string(),
            expires_at: 0,
        });
        if !entry.active(now) || entry.owner_id == owner_id {
            entry.owner_id = owner_id.to_string();
            entry.owner_name = owner_name.to_string();
            entry.expires_at = now + LEASE_MS;
        }
        entry.view()
    }

    /// Extend an own lease, or report the active foreign holder. `None`
    /// means there is nothing to renew; the expired entry is dropped.
    fn heartbeat(&self, workflow_id: &str, owner_id: &str) -> Option<LockView> {
        let now = now_ms();
        let mut leases = self.leases.lock().expect("lock map mutex");
        match leases.get_mut(workflow_id) {
            Some(lease) if lease.active(now) => {
                if lease.owner_id == owner_id {
                    lease.expires_at = now + LEASE_MS;
                }
                Some(lease.view())
            }
            Some(_) => {
                leases.remove(workflow_id);
                None
            }
            None => None,
        }
    }

    /// Drop the lease when held by `owner_id` (or already expired); a lease
    /// held by someone else is left untouched. Returns the remaining holder.
    fn release(&self, workflow_id: &str, owner_id: &str) -> Option<LockView> {
        let now = now_ms();
        let mut leases = self.leases.lock().expect("lock map mutex");
        let removable = leases
            .get(workflow_id)
            .is_some_and(|lease| lease.owner_id == owner_id || !lease.active(now));
        if removable {
            leases.remove(workflow_id);
        }
        leases
            .get(workflow_id)
            .filter(|lease| lease.active(now))
            .map(Lease::view)
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct LockView {
    owner_id: String,
    owner_name: String,
    expires_at: u64,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct AcquireLockBody {
    owner_id: String,
    owner_name: String,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct OwnerBody {
    owner_id: String,
}

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/workflows/{id}/lock", get(handle_get_lock))
        .route("/workflows/{id}/lock/acquire", post(handle_acquire_lock))
        .route(
            "/workflows/{id}/lock/heartbeat",
            post(handle_heartbeat_lock),
        )
        .route("/workflows/{id}/lock/release", post(handle_release_lock))
}

#[utoipa::path(
    get,
    path = "/api/v1/workflows/{id}/lock",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Current lease holder, null when unlocked", body = crate::envelope::ApiEnvelope<Option<crate::api::workflow::locks::LockView>>), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_get_lock(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    ok(state.locks.current(&path.id, now_ms())).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/lock/acquire",
    tag = "workflow",
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
    ok(state
        .locks
        .acquire(&path.id, &body.owner_id, &body.owner_name))
    .into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/lock/heartbeat",
    tag = "workflow",
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
    match state.locks.heartbeat(&path.id, &body.owner_id) {
        Some(view) => ok(view).into_response(),
        None => err(ApiError::validation("no active lease to renew")).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/workflows/{id}/lock/release",
    tag = "workflow",
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
    ok(state.locks.release(&path.id, &body.owner_id)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_is_idempotent_for_holder_and_blocks_others() {
        let locks = WorkflowLocks::default();
        let mine = locks.acquire("wf", "a", "Alice");
        assert_eq!(mine.owner_id, "a");
        let stolen = locks.acquire("wf", "b", "Bob");
        assert_eq!(stolen.owner_id, "a");
        let renewed = locks.acquire("wf", "a", "Alice");
        assert_eq!(renewed.owner_id, "a");
    }

    #[test]
    fn heartbeat_renews_only_the_holders_lease() {
        let locks = WorkflowLocks::default();
        locks.acquire("wf", "a", "Alice");
        let by_owner = locks.heartbeat("wf", "a").expect("active lease");
        assert_eq!(by_owner.owner_id, "a");
        let by_other = locks.heartbeat("wf", "b").expect("active lease");
        assert_eq!(by_other.owner_id, "a");
        assert!(locks.heartbeat("missing", "a").is_none());
    }

    #[test]
    fn release_ignores_foreign_leases() {
        let locks = WorkflowLocks::default();
        locks.acquire("wf", "a", "Alice");
        let after_foreign = locks.release("wf", "b");
        assert_eq!(after_foreign.expect("still held").owner_id, "a");
        let after_own = locks.release("wf", "a");
        assert!(after_own.is_none());
    }
}

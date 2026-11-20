//! Execution recovery bundle export: one self-contained archive per execution
//! for local backup, remote synchronization and restoration. The handler is a
//! thin transport adapter over `wf_api::execution_bundle`.

use axum::extract::{Extension, Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/executions/{id}/bundle", get(handle_execution_bundle))
        .route(
            "/remote-sync/executions/{id}/bundle",
            get(handle_remote_sync_bundle),
        )
        .route("/remote-sync/inventory", get(handle_remote_sync_inventory))
        .route("/remote-sync/reconcile", get(handle_remote_sync_reconcile))
        .route(
            "/remote-sync/revisions/rebuild",
            post(handle_remote_sync_rebuild),
        )
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ExecutionBundleQuery {
    /// Mark the exported bundle as checkpoint-synced.
    checkpoint_synced: Option<bool>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct RemoteInventoryQuery {
    /// Comma-separated execution ids. When omitted, every owned execution of
    /// the requested directory is reported.
    ids: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct RemoteSyncInventoryEntry {
    pub execution_id: String,
    pub found: bool,
    /// Monotonic sequence from the durable revision table.
    pub revision: Option<u64>,
    pub digest: Option<String>,
    pub updated_at_ms: Option<i64>,
    /// `ok`, `remote_missing`, `incomplete` or `needs_rebuild`.
    pub status: String,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct RemoteSyncInventory {
    pub executions: Vec<RemoteSyncInventoryEntry>,
}

#[derive(serde::Serialize)]
pub(crate) struct RemoteReconcileEntry {
    pub execution_id: String,
    pub status: String,
    pub sequence: Option<u64>,
    pub recorded_digest: Option<String>,
    pub exported_digest: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct RemoteReconcileResult {
    pub executions: Vec<RemoteReconcileEntry>,
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/bundle",
    operation_id = "get_executions_id_bundle",
    tag = "observation",
    params(IdPath, ExecutionBundleQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_execution_bundle(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
    Query(query): Query<ExecutionBundleQuery>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return crate::envelope::error_response(e);
    }

    match wf_api::execution_bundle::export_bundle(
        &state.ctx,
        &path.id,
        query.checkpoint_synced.unwrap_or(false),
    )
    .await
    {
        Ok(bundle) => ok(bundle).into_response(),
        Err(e) => error_response(e),
    }
}

/// One-response recovery snapshot for the local management service. The
/// execution records and every derived view share one owner check so a
/// backup never mixes data from different executions.
#[utoipa::path(
    get,
    path = "/api/v1/remote-sync/executions/{id}/bundle",
    operation_id = "get_remote_sync_bundle",
    tag = "observation",
    params(IdPath),
    responses((status = 200, description = "Complete execution recovery bundle", body = serde_json::Value), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_remote_sync_bundle(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    if let Err(e) = crate::directory::require_execution_owner(&state.ctx, &scope, &path.id).await {
        return error_response(e);
    }
    match wf_api::execution_bundle::export_bundle(&state.ctx, &path.id, false).await {
        Ok(bundle) => ok(bundle).into_response(),
        Err(e) => error_response(e),
    }
}

/// Revision inventory used by the management service to decide whether a
/// target needs a fresh snapshot or has disappeared from the remote node.
#[utoipa::path(
    get,
    path = "/api/v1/remote-sync/inventory",
    operation_id = "get_remote_sync_inventory",
    tag = "observation",
    params(RemoteInventoryQuery),
    responses((status = 200, description = "Execution revision inventory", body = serde_json::Value), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_remote_sync_inventory(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Query(query): Query<RemoteInventoryQuery>,
) -> impl IntoResponse {
    let wanted: Vec<String> = query
        .ids
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let directory = crate::directory::scope_dir_string(&scope);
    let listed = match wf_api::execution_listing::list_unified(
        &state.ctx,
        Some(&wf_api::execution_listing::UnifiedExecutionFilter {
            directory: directory.clone(),
            ..Default::default()
        }),
        usize::MAX / 2,
        0,
    )
    .await
    {
        Ok((rows, _)) => rows,
        Err(e) => return error_response(e),
    };
    let known: Vec<String> = if wanted.is_empty() {
        listed.iter().map(|row| row.execution_id.clone()).collect()
    } else {
        wanted
    };
    let needs_rebuild = !listed.is_empty()
        && state
            .ctx
            .storage
            .revisions
            .has_rows()
            .await
            .map(|found| !found)
            .unwrap_or(false);
    let mut executions = Vec::with_capacity(known.len());
    for execution_id in known {
        let found = listed.iter().any(|row| row.execution_id == execution_id);
        let mut revision = None;
        let mut digest = None;
        let mut updated_at_ms = None;
        let mut error = None;
        let status = if !found {
            "remote_missing"
        } else if needs_rebuild {
            "needs_rebuild"
        } else {
            match wf_api::execution_bundle::revision_snapshot(&state.ctx, &execution_id).await {
                Ok(row) => {
                    revision = Some(row.sequence);
                    digest = Some(row.digest);
                    updated_at_ms = Some(row.updated_at_ms);
                    "ok"
                }
                Err(e) => {
                    error = Some(e.to_string());
                    "incomplete"
                }
            }
        };
        executions.push(RemoteSyncInventoryEntry {
            execution_id,
            found,
            revision,
            digest,
            updated_at_ms,
            status: status.to_string(),
            error,
        });
    }
    ok(RemoteSyncInventory { executions }).into_response()
}

fn reconcile_ids(query: &RemoteInventoryQuery) -> Vec<String> {
    query
        .ids
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Full-export verification of suspicious targets. Inventory stays O(1);
/// this slower path runs only from startup or explicit reconciliation.
#[utoipa::path(
    get,
    path = "/api/v1/remote-sync/reconcile",
    operation_id = "get_remote_sync_reconcile",
    tag = "observation",
    params(RemoteInventoryQuery),
    responses((status = 200, description = "Revision verification", body = serde_json::Value), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_remote_sync_reconcile(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    Query(query): Query<RemoteInventoryQuery>,
) -> impl IntoResponse {
    let directory = crate::directory::scope_dir_string(&scope);
    let listed = match wf_api::execution_listing::list_unified(
        &state.ctx,
        Some(&wf_api::execution_listing::UnifiedExecutionFilter {
            directory: directory.clone(),
            ..Default::default()
        }),
        usize::MAX / 2,
        0,
    )
    .await
    {
        Ok((rows, _)) => rows,
        Err(e) => return error_response(e),
    };
    let known = {
        let wanted = reconcile_ids(&query);
        if wanted.is_empty() {
            listed.iter().map(|row| row.execution_id.clone()).collect()
        } else {
            wanted
        }
    };
    let mut executions = Vec::with_capacity(known.len());
    for execution_id in known {
        let mut entry = RemoteReconcileEntry {
            execution_id: execution_id.clone(),
            status: "ok".to_string(),
            sequence: None,
            recorded_digest: None,
            exported_digest: None,
        };
        if !listed.iter().any(|row| row.execution_id == execution_id) {
            entry.status = "remote_missing".to_string();
            executions.push(entry);
            continue;
        }
        let revision = state
            .ctx
            .storage
            .revisions
            .load(&execution_id)
            .await
            .ok()
            .flatten();
        match revision.as_ref() {
            Some(revision) => {
                entry.sequence = Some(revision.sequence);
                entry.recorded_digest = Some(revision.digest.clone());
            }
            // A missing row still gets a full export: the contents may be
            // healthy, and the manager needs the exported digest to rebuild
            // only this row instead of the whole table.
            None => entry.status = "incomplete".to_string(),
        }
        match wf_api::execution_bundle::export_bundle(&state.ctx, &execution_id, false).await {
            Ok(bundle) => {
                entry.exported_digest = Some(bundle.payload_digest.clone());
                if let Some(revision) = revision.as_ref() {
                    if bundle.payload_digest != revision.digest {
                        entry.status = "digest_mismatch".to_string();
                    }
                }
            }
            Err(e) => {
                entry.status = "export_failed".to_string();
                entry.exported_digest = Some(e.to_string());
            }
        }
        executions.push(entry);
    }
    ok(RemoteReconcileResult { executions }).into_response()
}

/// One revision row to rebuild plus the sequence the manager already tracks
/// for that target.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RemoteRebuildTarget {
    pub execution_id: String,
    /// Sequence the caller already uses as its comparison baseline. The
    /// rebuilt row reuses it so a rebuild never reads as a rollback.
    #[serde(default)]
    pub minimum_sequence: u64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RemoteRebuildRequest {
    /// Explicit targets. An empty list rebuilds every execution of the
    /// requested directory from sequence one.
    #[serde(default)]
    pub targets: Vec<RemoteRebuildTarget>,
}

/// Rebuild revision rows from full exports after a row or the table is lost.
#[utoipa::path(
    post,
    path = "/api/v1/remote-sync/revisions/rebuild",
    operation_id = "post_remote_sync_revisions_rebuild",
    tag = "observation",
    request_body = RemoteRebuildRequest,
    responses((status = 200, description = "Rebuilt revisions", body = serde_json::Value), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_remote_sync_rebuild(
    State(state): State<ApiState>,
    Extension(scope): Extension<Option<crate::directory::DirectoryScope>>,
    axum::Json(body): axum::Json<RemoteRebuildRequest>,
) -> impl IntoResponse {
    let directory = crate::directory::scope_dir_string(&scope);
    let targets: Vec<(String, u64)> = if body.targets.is_empty() {
        match wf_api::execution_listing::list_unified(
            &state.ctx,
            Some(&wf_api::execution_listing::UnifiedExecutionFilter {
                directory,
                ..Default::default()
            }),
            usize::MAX / 2,
            0,
        )
        .await
        {
            Ok((rows, _)) => rows.into_iter().map(|row| (row.execution_id, 0)).collect(),
            Err(e) => return error_response(e),
        }
    } else {
        body.targets
            .into_iter()
            .map(|target| (target.execution_id, target.minimum_sequence))
            .collect()
    };
    let mut rebuilt = Vec::new();
    for (execution_id, minimum_sequence) in targets {
        let result =
            wf_api::execution_bundle::rebuild_revision(&state.ctx, &execution_id, minimum_sequence)
                .await;
        rebuilt.push(serde_json::json!({
            "execution_id": execution_id,
            "minimum_sequence": minimum_sequence,
            "sequence": result.as_ref().ok().map(|row| row.sequence),
            "error": result.as_ref().err().map(|e| e.to_string()),
        }));
    }
    ok(serde_json::json!({ "executions": rebuilt })).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use axum::response::Response;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;

    pub(super) fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    pub(super) async fn save_agent(ctx: &ApiContext, id: &str) {
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from(id.to_string()),
            definition_id: wf_types::Id::from("agent-1".to_string()),
            execution_type: wf_types::execution::ExecutionType::Agent,
            status: wf_types::ExecutionStatus::Completed,
            current_iteration: 1,
            tool_call_count: 0,
            iteration_history: Some(vec![wf_types::agent_execution::IterationRecord {
                iteration: 1,
                started_at: 10,
                completed_at: Some(20),
                tool_calls: None,
                response_content: None,
                llm_calls: None,
                error: None,
            }]),
            started_at: 1,
            completed_at: Some(30),
            error: None,
            context: None,
            loop_config: None,
            permanently_failed_tools: None,
            hierarchy: None,
            rerun_of: None,
            directory: None,
        };
        ctx.storage.agent_execution.save(&record).await.unwrap();
    }

    pub(super) async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    pub(super) async fn json_body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn bundle_exports_a_known_execution() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-bundle").await;

        let response = send(ctx, "/api/v1/executions/loop-bundle/bundle").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["version"], 1);
        assert_eq!(body["data"]["execution_id"], "loop-bundle");
        assert_eq!(body["data"]["checkpoint_synced"], false);
        assert!(body["data"]["payload_digest"].as_str().is_some());
    }

    #[tokio::test]
    async fn remote_sync_reports_revision() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-sync").await;
        let response = send(ctx, "/api/v1/remote-sync/inventory?ids=loop-sync,missing").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let rows = body["data"]["executions"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["execution_id"], "loop-sync");
        assert_eq!(rows[0]["found"], true);
        assert_eq!(rows[0]["status"], "ok");
        assert!(rows[0]["revision"].as_u64().is_some_and(|value| value > 0));
        assert!(rows[0]["digest"].as_str().is_some());
        assert_eq!(rows[1]["found"], false);
        assert_eq!(rows[1]["status"], "remote_missing");
    }
}

#[cfg(test)]
mod revision_tests {
    use super::tests::{json_body, make_ctx, save_agent, send};

    #[tokio::test]
    async fn inventory_reports_needs_rebuild_when_revision_table_is_lost() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-rebuild").await;
        ctx.storage.revisions.clear().await.unwrap();

        let body =
            json_body(send(ctx, "/api/v1/remote-sync/inventory?ids=loop-rebuild").await).await;
        let rows = body["data"]["executions"].as_array().expect("rows");
        assert_eq!(rows[0]["found"], true);
        assert_eq!(rows[0]["status"], "needs_rebuild");
        assert!(rows[0]["revision"].is_null());
    }

    #[tokio::test]
    async fn reconcile_reports_a_digest_mismatch_without_writing() {
        let ctx = make_ctx();
        save_agent(&ctx, "loop-mismatch").await;
        let before = ctx
            .storage
            .revisions
            .load("loop-mismatch")
            .await
            .unwrap()
            .unwrap();
        ctx.storage
            .revisions
            .advance_file_archive("loop-mismatch", "forged-archive".to_string())
            .await
            .unwrap();
        let after = ctx
            .storage
            .revisions
            .load("loop-mismatch")
            .await
            .unwrap()
            .unwrap();
        assert!(after.sequence > before.sequence);

        let body =
            json_body(send(ctx, "/api/v1/remote-sync/reconcile?ids=loop-mismatch").await).await;
        let rows = body["data"]["executions"].as_array().expect("rows");
        assert_eq!(rows[0]["status"], "digest_mismatch");
        assert_eq!(rows[0]["recorded_digest"], after.digest);
    }
}

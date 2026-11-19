//! Execution recovery bundle export: one self-contained archive per execution
//! for local backup, remote synchronization and restoration. The handler is a
//! thin transport adapter over `wf_api::execution_bundle`.

use axum::extract::{Extension, Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
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
    pub revision: Option<String>,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct RemoteSyncInventory {
    pub executions: Vec<RemoteSyncInventoryEntry>,
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
    let mut executions = Vec::with_capacity(known.len());
    for execution_id in known {
        let found = listed.iter().any(|row| row.execution_id == execution_id);
        let mut revision = None;
        let mut error = None;
        if found {
            match wf_api::execution_bundle::source_revision(&state.ctx, &execution_id).await {
                Ok(value) => revision = Some(value),
                Err(e) => error = Some(e.to_string()),
            }
        }
        executions.push(RemoteSyncInventoryEntry {
            execution_id,
            found,
            revision,
            error,
        });
    }
    ok(RemoteSyncInventory { executions }).into_response()
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

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    async fn save_agent(ctx: &ApiContext, id: &str) {
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

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
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
        assert!(rows[0]["revision"].as_str().is_some());
        assert_eq!(rows[1]["found"], false);
    }
}

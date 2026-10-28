//! Execution domain: externally submitted workflow variable injections.
//!
//! Updates submitted here never touch a running node. They wait in the
//! execution's pending buffer and land on the live variable map at a node
//! boundary (`next_boundary`) or during an explicit pause/resume cycle
//! (`on_resume`).

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::extract::{IdInjectionPath, IdPath};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/executions/{id}/variable-injections",
            post(handle_submit_variable_injection).get(handle_list_variable_injections),
        )
        .route(
            "/executions/{id}/variable-injections/{injection_id}",
            delete(handle_delete_variable_injection),
        )
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SubmitVariableInjectionBody {
    /// Top-level variable name. The `__` prefix and dotted paths are rejected.
    pub(crate) variable_name: String,
    /// New value applied at the boundary. Stored verbatim, without the
    /// type coercion a `VARIABLE` node would perform.
    pub(crate) value: serde_json::Value,
    /// Audit source label (`user` / `cli` / `tui` / `web` /
    /// `workflow-console`); absent defaults to `api`.
    #[serde(default)]
    pub(crate) source: Option<String>,
    /// Application policy (`next_boundary` default, `on_resume`).
    #[serde(default)]
    pub(crate) policy: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct VariableInjectionView {
    pub(crate) injection_id: String,
    pub(crate) variable_name: String,
    pub(crate) value: serde_json::Value,
    pub(crate) source: String,
    pub(crate) submitted_at: i64,
    pub(crate) policy: String,
}

fn view_of(
    item: wf_api::workflow::variable_injection::PendingVariableInjection,
) -> VariableInjectionView {
    VariableInjectionView {
        injection_id: item.id,
        variable_name: item.variable_name,
        value: item.value,
        source: item.source,
        submitted_at: item.submitted_at,
        policy: item.policy.label().to_string(),
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ListVariableInjectionsView {
    pub(crate) injections: Vec<VariableInjectionView>,
}

#[utoipa::path(
    post,
    path = "/api/v1/executions/{id}/variable-injections",
    tag = "workflow",
    params(IdPath),
    request_body = SubmitVariableInjectionBody,
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::workflow::variable_injections::VariableInjectionView>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_submit_variable_injection(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<SubmitVariableInjectionBody>,
) -> impl IntoResponse {
    match wf_api::workflow::submit_variable_injection(
        &state.ctx,
        &path.id,
        body.variable_name,
        body.value,
        body.source.as_deref(),
        body.policy.as_deref(),
    )
    .await
    {
        Ok(injection) => ok(view_of(injection)).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/executions/{id}/variable-injections",
    tag = "workflow",
    params(IdPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::workflow::variable_injections::ListVariableInjectionsView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_list_variable_injections(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::list_variable_injections(&state.ctx, &path.id).await {
        Ok(injections) => ok(ListVariableInjectionsView {
            injections: injections.into_iter().map(view_of).collect(),
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct DeleteVariableInjectionView {
    pub(crate) removed: bool,
}

#[utoipa::path(
    delete,
    path = "/api/v1/executions/{id}/variable-injections/{injection_id}",
    tag = "workflow",
    params(IdInjectionPath),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::api::workflow::variable_injections::DeleteVariableInjectionView>), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_delete_variable_injection(
    State(state): State<ApiState>,
    Path(path): Path<IdInjectionPath>,
) -> impl IntoResponse {
    match wf_api::workflow::remove_variable_injection(&state.ctx, &path.id, &path.injection_id)
        .await
    {
        Ok(()) => ok(DeleteVariableInjectionView { removed: true }).into_response(),
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;
    use wf_core::registry::MutableRegistry;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    fn register_live_execution(ctx: &Arc<ApiContext>, id: &str) {
        let entity = Arc::new(wf_workflow::entity::WorkflowExecutionEntity::new(
            id.to_string(),
            "wf-1".to_string(),
        ));
        let _ = ctx.workflow_executions.register(id.to_string(), entity);
    }

    async fn send(
        ctx: Arc<ApiContext>,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> axum::response::Response {
        let builder = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(json) => builder
                .header("content-type", "application/json")
                .body(AxBody::from(serde_json::to_vec(&json).unwrap()))
                .unwrap(),
            None => builder.body(AxBody::empty()).unwrap(),
        };
        crate::router::api_router(ctx)
            .oneshot(request)
            .await
            .unwrap()
    }

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn submit_body(name: &str) -> serde_json::Value {
        serde_json::json!({
            "variable_name": name,
            "value": 7,
            "source": "cli",
            "policy": "next_boundary",
        })
    }

    #[tokio::test]
    async fn unknown_execution_maps_to_not_found() {
        let ctx = make_ctx();
        let response = send(
            ctx.clone(),
            "POST",
            "/api/v1/executions/missing/variable-injections",
            Some(submit_body("speed")),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = send(
            ctx.clone(),
            "GET",
            "/api/v1/executions/missing/variable-injections",
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = send(
            ctx,
            "DELETE",
            "/api/v1/executions/missing/variable-injections/abc",
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn submit_list_and_delete_round_trip() {
        let ctx = make_ctx();
        register_live_execution(&ctx, "exec-1");

        let response = send(
            ctx.clone(),
            "POST",
            "/api/v1/executions/exec-1/variable-injections",
            Some(submit_body("speed")),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["variable_name"], "speed");
        assert_eq!(body["data"]["value"], 7);
        assert_eq!(body["data"]["source"], "cli");
        assert_eq!(body["data"]["policy"], "next_boundary");
        let injection_id = body["data"]["injection_id"].as_str().unwrap().to_string();

        let response = send(
            ctx.clone(),
            "GET",
            "/api/v1/executions/exec-1/variable-injections",
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["injections"].as_array().unwrap().len(), 1);

        let response = send(
            ctx.clone(),
            "DELETE",
            &format!("/api/v1/executions/exec-1/variable-injections/{injection_id}"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["data"]["removed"], true);

        let response = send(
            ctx.clone(),
            "GET",
            "/api/v1/executions/exec-1/variable-injections",
            None,
        )
        .await;
        let body = json_body(response).await;
        assert_eq!(body["data"]["injections"].as_array().unwrap().len(), 0);

        // Deleting again reports NotFound instead of a silent success.
        let response = send(
            ctx,
            "DELETE",
            &format!("/api/v1/executions/exec-1/variable-injections/{injection_id}"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_submissions_are_rejected() {
        let ctx = make_ctx();
        register_live_execution(&ctx, "exec-1");
        for body in [
            serde_json::json!({"variable_name": "", "value": 1}),
            serde_json::json!({"variable_name": "__secret", "value": 1}),
            serde_json::json!({"variable_name": "a.b", "value": 1}),
            serde_json::json!({"variable_name": "x", "value": 1, "source": "evil"}),
            serde_json::json!({"variable_name": "x", "value": 1, "policy": "later"}),
        ] {
            let response = send(
                ctx.clone(),
                "POST",
                "/api/v1/executions/exec-1/variable-injections",
                Some(body),
            )
            .await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }
}

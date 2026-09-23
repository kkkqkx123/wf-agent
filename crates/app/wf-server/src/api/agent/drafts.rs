//! Agent draft lifecycle: editable agent definitions that promote into the
//! formal agent template registry. Thin transport adapters over
//! `wf-api::agent::agent_draft`.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/agents/drafts",
            get(handle_list_drafts).post(handle_save_draft),
        )
        .route(
            "/agents/drafts/{id}",
            get(handle_get_draft).delete(handle_delete_draft),
        )
        .route("/agents/drafts/{id}/promote", post(handle_promote_draft))
        .route("/agents/drafts/{id}/validate", get(handle_validate_draft))
        .route("/agents/{id}/lifecycle", get(handle_lifecycle))
}

#[utoipa::path(
    get,
    path = "/agents/drafts",
    tag = "agent",
    responses(
        (status = 200, description = "List of agent drafts", body = serde_json::Value),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_list_drafts(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::agent::agent_draft::list_drafts(&state.ctx).await {
        Ok(drafts) => ok(drafts).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/agents/drafts",
    tag = "agent",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Agent draft created", body = String),
        (status = 400, description = "Invalid request body", body = crate::envelope::ErrorResponse),
        (status = 409, description = "Agent draft already exists", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_save_draft(
    State(state): State<ApiState>,
    Json(definition): Json<wf_api::AgentDefinition>,
) -> impl IntoResponse {
    match wf_api::agent::agent_draft::save_draft(&state.ctx, &definition).await {
        Ok(()) => ok(definition.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agents/drafts/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Agent draft ID")),
    responses(
        (status = 200, description = "Agent draft found", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_get_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_draft::get_draft(&state.ctx, &path.id).await {
        Ok(draft) => ok(draft).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/agents/drafts/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Agent draft ID")),
    responses(
        (status = 200, description = "Agent draft deleted", body = bool),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_delete_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_draft::delete_draft(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/agents/drafts/{id}/promote",
    tag = "agent",
    params(("id" = String, Path, description = "Agent draft ID")),
    responses(
        (status = 200, description = "Draft promoted", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_promote_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_draft::promote_draft(&state.ctx, &path.id).await {
        Ok(warnings) => ok(warnings).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agents/drafts/{id}/validate",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Draft validation", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_validate_draft(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_draft::validate_draft_complete(&state.ctx, &path.id).await {
        Ok(warnings) => ok(warnings).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/agents/{id}/lifecycle",
    tag = "agent",
    params(("id" = String, Path, description = "Agent loop ID")),
    responses(
        (status = 200, description = "Agent lifecycle", body = serde_json::Value),
        (status = 404, description = "Not found", body = crate::envelope::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_lifecycle(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    ok(wf_api::agent::agent_draft::lifecycle_of(&state.ctx, &path.id).await).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    fn draft_agent(id: &str) -> wf_api::AgentDefinition {
        wf_api::AgentDefinition {
            id: id.into(),
            name: format!("Draft {id}"),
            description: None,
            version: None,
            config: None,
            metadata: None,
            created_at: wf_api::now(),
            updated_at: wf_api::now(),
        }
    }

    async fn send(
        ctx: Arc<ApiContext>,
        method: &str,
        uri: &str,
        body: Option<String>,
    ) -> StatusCode {
        let builder = Request::builder().method(method).uri(uri);
        let request = if let Some(body) = body {
            builder
                .header("content-type", "application/json")
                .body(AxBody::from(body))
                .unwrap()
        } else {
            builder.body(AxBody::empty()).unwrap()
        };
        crate::router::api_router(ctx)
            .oneshot(request)
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn agent_draft_lifecycle_round_trip() {
        let ctx = make_ctx();
        let body = serde_json::to_string(&draft_agent("agent-draft")).unwrap();
        assert_eq!(
            send(ctx.clone(), "POST", "/api/v1/agents/drafts", Some(body)).await,
            StatusCode::OK
        );
        assert_eq!(
            send(ctx.clone(), "GET", "/api/v1/agents/draft-missing", None).await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(
                ctx.clone(),
                "POST",
                "/api/v1/agents/drafts/agent-draft/promote",
                None
            )
            .await,
            StatusCode::OK
        );
    }
}

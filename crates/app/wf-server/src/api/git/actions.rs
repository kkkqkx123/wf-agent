use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/git/action", post(handle_git_action))
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct GitActionBody {
    action: String,
    #[serde(default)]
    params: std::collections::HashMap<String, String>,
    #[serde(default)]
    confirm: Option<String>,
}

#[utoipa::path(post, path = "/api/v1/git/action", operation_id = "post_git_action", tag = "git", request_body = GitActionBody, responses((status = 200, description = "Action outcome with command echo", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Unknown action or missing params", body = crate::envelope::ErrorResponse), (status = 409, description = "Unavailable or confirm required", body = crate::envelope::ErrorResponse)), security(("api_key" = [])))]
pub(crate) async fn handle_git_action(
    State(state): State<ApiState>,
    axum::Json(body): axum::Json<GitActionBody>,
) -> impl IntoResponse {
    let request = wf_api::git::ActionRequest {
        action: body.action,
        params: body.params,
        confirm: body.confirm,
    };
    match wf_api::git::execute_action(&state.ctx, &request) {
        Ok(outcome) => {
            // The readonly `refresh` entry only re-reads state; every other
            // success mutated the repository and wakes fingerprint listeners.
            if request.action.trim() != "refresh" {
                publish_git_changed(&state.ctx, request.action.trim()).await;
            }
            ok(outcome).into_response()
        }
        Err(err) => error_response(err.into()),
    }
}

/// Publish a repository transition on the shared event bus. Pushes carry
/// only the transition; the fingerprint stays the cheap change guard over
/// REST. Bus failures never fail the action response.
async fn publish_git_changed(ctx: &wf_api::ApiContext, action: &str) {
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("source".to_string(), serde_json::json!("action"));
    metadata.insert("action".to_string(), serde_json::json!(action));
    let event = wf_types::events::BaseEvent {
        id: wf_common::generate_id(),
        r#type: wf_types::events::EventType::GitChanged,
        timestamp: wf_api::now(),
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,
        event_name: Some("git.changed".to_string()),
        metadata: Some(metadata),
    };
    if let Err(err) = wf_api::infra::events::dispatch(ctx, event).await {
        tracing::warn!(target: "wf_server", %action, %err, "git change event dispatch failed");
    }
}

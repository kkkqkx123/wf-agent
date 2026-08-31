//! Agent trigger and interaction surfaces: global trigger registry, enable /
//! disable, statistics and user interactions. Handlers are thin transport
//! adapters over the `wf-api::entity` trigger and interaction surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_types::TriggerStorageMetadata;

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, TidPath};
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── agent triggers (global, event-driven resources) ──
        .route(
            "/agent-triggers",
            get(handle_list_triggers).post(handle_register_trigger),
        )
        .route("/agent-triggers/{tid}", get(handle_get_trigger))
        .route("/agent-triggers/export", get(handle_trigger_export))
        .route("/agent-triggers/history", get(handle_trigger_history))
        .route("/agent-triggers/{tid}/enable", post(handle_trigger_enable))
        .route(
            "/agent-triggers/{tid}/disable",
            post(handle_trigger_disable),
        )
        .route("/agent-triggers/{tid}/enabled", get(handle_trigger_enabled))
        .route("/agent-triggers/stats", get(handle_trigger_statistics))
        // ── agent interactions ──
        .route(
            "/agent-loops/{id}/interactions",
            get(handle_list_interactions),
        )
        .route("/agent-interactions/{id}", get(handle_get_interaction))
        .route(
            "/agent-interactions/{id}/respond",
            post(handle_respond_interaction),
        )
}

// ── agent triggers ────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListTriggersQuery {
    event: Option<String>,
}

async fn handle_list_triggers(
    State(state): State<ApiState>,
    Query(query): Query<ListTriggersQuery>,
) -> impl IntoResponse {
    let result = match query.event.as_deref() {
        Some(event) => {
            wf_api::entity::trigger::list_triggers_by_event(&state.ctx.storage, event).await
        }
        None => wf_api::entity::trigger::list_triggers(&state.ctx.storage, None).await,
    };
    match result {
        Ok(triggers) => ok(triggers).into_response(),
        Err(e) => error_response(e),
    }
}

/// Register a new trigger (agent executions reference triggers by name; a
/// duplicate id is rejected with AlreadyExists). Triggers are global,
/// event-driven resources — no loop scoping.
async fn handle_register_trigger(
    State(state): State<ApiState>,
    Json(trigger): Json<TriggerStorageMetadata>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::register_trigger(&state.ctx.storage, &trigger).await {
        Ok(()) => ok(trigger.id.to_string()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_trigger(
    State(state): State<ApiState>,
    Path(path): Path<TidPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::get_trigger(&state.ctx.storage, &path.tid).await {
        Ok(trigger) => ok(trigger).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_export(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::trigger::export_triggers(&state.ctx.storage).await {
        Ok(export) => ok(export).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct TriggerHistoryQuery {
    execution_id: String,
    trigger_name: Option<String>,
}

async fn handle_trigger_history(
    State(state): State<ApiState>,
    Query(query): Query<TriggerHistoryQuery>,
) -> impl IntoResponse {
    match wf_api::entity::trigger_execution::execution_history(
        &state.ctx.storage,
        &query.execution_id,
        query.trigger_name.as_deref(),
    )
    .await
    {
        Ok(history) => ok(history).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_enable(
    State(state): State<ApiState>,
    Path(path): Path<TidPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::enable_trigger(&state.ctx.storage, &path.tid).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_disable(
    State(state): State<ApiState>,
    Path(path): Path<TidPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::disable_trigger(&state.ctx.storage, &path.tid).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

/// Query the persisted enabled state of an agent trigger.
async fn handle_trigger_enabled(
    State(state): State<ApiState>,
    Path(path): Path<TidPath>,
) -> impl IntoResponse {
    match wf_api::entity::trigger::is_trigger_enabled(&state.ctx.storage, &path.tid).await {
        Ok(enabled) => ok(enabled).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_trigger_statistics(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::entity::trigger::trigger_fire_statistics(&state.ctx.storage).await {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

// ── agent interactions ────────────────────────────────────────────

#[derive(Deserialize)]
struct ListInteractionsQuery {
    status: Option<String>,
    limit: Option<usize>,
}

async fn handle_list_interactions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListInteractionsQuery>,
) -> impl IntoResponse {
    match wf_api::agent::agent_user_interaction::list_filtered(
        &state.ctx,
        &path.id,
        query.status.as_deref(),
        query.limit,
    )
    .await
    {
        Ok(interactions) => ok(interactions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_get_interaction(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::agent::agent_user_interaction::get(&state.ctx, &path.id).await {
        Ok(interaction) => ok(interaction).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct AgentRespondBody {
    agent_loop_id: Option<String>,
    response_data: Option<Value>,
    result_data: Option<Value>,
}

async fn handle_respond_interaction(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<AgentRespondBody>,
) -> impl IntoResponse {
    let Some(agent_loop_id) = body.agent_loop_id else {
        return crate::envelope::err::<Value>(crate::envelope::ApiError::validation(
            "agent_loop_id is required to respond to an agent interaction",
        ))
        .into_response();
    };
    match wf_api::agent::agent_user_interaction::respond(
        &state.ctx,
        &agent_loop_id,
        &path.id,
        body.response_data,
        body.result_data,
    )
    .await
    {
        Ok(()) => ok(()).into_response(),
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

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        ))
    }

    fn trigger_body(id: &str, name: &str) -> String {
        serde_json::json!({
            "id": id,
            "name": name,
            "description": "test trigger",
            "event": "on_node_completed",
            "enabled": true,
            "created_at": 1000,
            "updated_at": 1000
        })
        .to_string()
    }

    #[tokio::test]
    async fn trigger_can_be_registered_and_rejects_duplicates() {
        let ctx = make_ctx();
        let register = |ctx: Arc<ApiContext>, body: String| {
            let ctx = ctx.clone();
            async move {
                crate::router::api_router(ctx)
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/api/v1/agent-triggers")
                            .header("content-type", "application/json")
                            .body(AxBody::from(body))
                            .unwrap(),
                    )
                    .await
                    .unwrap()
            }
        };

        let created = register(ctx.clone(), trigger_body("trg-1", "trg-1")).await;
        assert_eq!(created.status(), StatusCode::OK, "register trigger");

        let duplicate = register(ctx.clone(), trigger_body("trg-1", "trg-1")).await;
        assert_eq!(
            duplicate.status(),
            StatusCode::CONFLICT,
            "duplicate trigger"
        );

        let listed = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/agent-triggers")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(listed.status(), StatusCode::OK, "list triggers");
    }
}

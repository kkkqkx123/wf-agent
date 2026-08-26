//! Execution state domain: live state views (variables / context /
//! transitions / agent views) and persisted state records (snapshots).
//! Handlers are thin transport adapters over the `wf-api` execution state
//! and `state_tracker` surfaces.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── state views ──
        .route("/executions/{id}/state", get(handle_state))
        .route("/executions/{id}/variables", get(handle_variables))
        .route("/executions/{id}/transitions", get(handle_transitions))
        .route("/executions/{id}/context", get(handle_context))
        .route("/executions/{id}/call-stack", get(handle_call_stack))
        .route("/executions/{id}/memory", get(handle_memory))
        .route(
            "/executions/{id}/variable-snapshots",
            get(handle_variable_snapshots),
        )
        .route(
            "/executions/{id}/context-evolution",
            get(handle_context_evolution),
        )
        .route(
            "/executions/{id}/state-analysis",
            get(handle_state_analysis),
        )
        .route(
            "/executions/{id}/context-transitions",
            get(handle_context_transitions),
        )
        .route(
            "/executions/{id}/context-snapshots",
            get(handle_key_context_snapshots),
        )
        .route(
            "/executions/{id}/agent-state",
            get(handle_agent_execution_state),
        )
        .route(
            "/executions/{id}/agent-iterations",
            get(handle_agent_execution_iterations),
        )
        .route(
            "/executions/{id}/agent-variables",
            get(handle_agent_execution_variables),
        )
        // ── state records (persisted snapshots) ──
        .route(
            "/executions/{id}/state-records",
            get(handle_state_records).delete(handle_clear_state_records),
        )
        .route(
            "/executions/{id}/state-records/iterations/{iteration}",
            get(handle_state_at_iteration),
        )
        .route(
            "/executions/{id}/state-records/snapshots/{timestamp}",
            get(handle_variable_snapshot_at),
        )
        .route(
            "/executions/{id}/state-records/variables/{name}/history",
            get(handle_state_variable_history),
        )
        .route(
            "/executions/{id}/state-records/most-changed",
            get(handle_most_changed_variables),
        )
        .route(
            "/executions/{id}/state-records/mutation-count",
            get(handle_variable_mutation_count),
        )
        .route(
            "/executions/{id}/state-records/call-stack",
            get(handle_state_call_stack),
        )
        .route(
            "/executions/{id}/state-records/memory",
            get(handle_state_memory),
        )
        .route(
            "/executions/{id}/state-records/memory/peak",
            get(handle_state_memory_peak),
        )
}

// ── state views ───────────────────────────────────────────────────

async fn handle_state(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_state(&state.ctx, &path.id)
        .await
    {
        Ok(view) => ok(view).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_variables(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_variables(&state.ctx, &path.id)
        .await
    {
        Ok(variables) => ok(variables).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_transitions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_status_transitions(
        &state.ctx, &path.id,
    )
    .await
    {
        Ok(transitions) => ok(transitions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_context(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_execution_context(
        &state.ctx, &path.id,
    )
    .await
    {
        Ok(context) => ok(context).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_call_stack(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_call_stack(&state.ctx, &path.id)
        .await
    {
        Ok(stack) => ok(stack).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_memory(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_memory_usage(
        &state.ctx, &path.id,
    )
    .await
    {
        Ok(memory) => ok(memory).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SnapshotRangeQuery {
    start: Option<i64>,
    end: Option<i64>,
}

async fn handle_variable_snapshots(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<SnapshotRangeQuery>,
) -> impl IntoResponse {
    let result = match (query.start, query.end) {
        (Some(start), Some(end)) => {
            wf_api::workflow::execution_state::workflow_execution_get_variable_snapshots_by_time_range(
                &state.ctx, &path.id, start, end,
            )
            .await
        }
        _ => wf_api::workflow::execution_state::workflow_execution_get_variable_snapshots(
            &state.ctx, &path.id,
        )
        .await,
    };
    match result {
        Ok(snapshots) => ok(snapshots).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_context_evolution(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_context_evolution(
        &state.ctx, &path.id,
    )
    .await
    {
        Ok(evolution) => ok(evolution).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_state_analysis(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_analyze_state_transitions(
        &state.ctx, &path.id,
    )
    .await
    {
        Ok(analysis) => ok(analysis).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_context_transitions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_context_transitions(
        &state.ctx, &path.id,
    )
    .await
    {
        Ok(transitions) => ok(transitions).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_key_context_snapshots(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::workflow_execution_get_key_context_snapshots(
        &state.ctx, &path.id,
    )
    .await
    {
        Ok(snapshots) => ok(snapshots).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_agent_execution_state(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::agent_execution_get_state(&state.ctx, &path.id).await {
        Ok(view) => ok(view).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_agent_execution_iterations(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::agent_execution_iteration_history(&state.ctx, &path.id)
        .await
    {
        Ok(records) => ok(records).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_agent_execution_variables(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::workflow::execution_state::agent_execution_variables(&state.ctx, &path.id).await {
        Ok(variables) => ok(variables).into_response(),
        Err(e) => error_response(e),
    }
}

// ── state records (persisted snapshots) ─────────────────────────

async fn handle_state_records(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::list_state_records(&state.ctx, &path.id).await {
        Ok(records) => ok(records).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_clear_state_records(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::clear_state(&state.ctx, &path.id).await {
        Ok(()) => ok(()).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct IterationPath {
    id: String,
    iteration: u32,
}

async fn handle_state_at_iteration(
    State(state): State<ApiState>,
    Path(path): Path<IterationPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::get_state_at_iteration(&state.ctx, &path.id, path.iteration)
        .await
    {
        Ok(record) => ok(record).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SnapshotAtPath {
    id: String,
    timestamp: i64,
}

async fn handle_variable_snapshot_at(
    State(state): State<ApiState>,
    Path(path): Path<SnapshotAtPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::get_variable_snapshot(&state.ctx, &path.id, path.timestamp)
        .await
    {
        Ok(snapshot) => ok(snapshot).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VariableHistoryPath {
    id: String,
    name: String,
}

async fn handle_state_variable_history(
    State(state): State<ApiState>,
    Path(path): Path<VariableHistoryPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::get_variable_history(&state.ctx, &path.id, &path.name).await
    {
        Ok(history) => ok(history).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct MostChangedQuery {
    limit: Option<usize>,
}

async fn handle_most_changed_variables(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<MostChangedQuery>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::get_most_changed_variables(
        &state.ctx,
        &path.id,
        query.limit.unwrap_or(10),
    )
    .await
    {
        Ok(variables) => ok(variables).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_variable_mutation_count(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::get_variable_mutation_count(&state.ctx, &path.id).await {
        Ok(count) => ok(count).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_state_call_stack(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::get_call_stack(&state.ctx, &path.id).await {
        Ok(stack) => ok(stack).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_state_memory(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::get_memory_usage(&state.ctx, &path.id).await {
        Ok(memory) => ok(memory).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_state_memory_peak(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::infra::state_tracker::get_peak_memory_usage(&state.ctx, &path.id).await {
        Ok(memory) => ok(memory).into_response(),
        Err(e) => error_response(e),
    }
}

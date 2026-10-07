//! Agent loop surface: CRUD, run control (run / stream / pause / resume /
//! cancel), status, summaries, iteration history and timeline. Handlers are
//! thin transport adapters over the `wf-api::agent` loop surfaces, grouped
//! into `crud` / `lifecycle` / `inspect` handler modules.

mod crud;
mod inspect;
mod lifecycle;

pub(crate) use crud::*;
pub(crate) use inspect::*;
pub(crate) use lifecycle::*;

use axum::routing::{get, patch, post};
use axum::Router;

use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        // ── agent loops ──
        .route(
            "/agent-loops",
            get(handle_list_loops).post(handle_save_loop),
        )
        .route(
            "/agent-loops/{id}",
            get(handle_get_loop)
                .put(handle_update_loop)
                .delete(handle_delete_loop),
        )
        .route("/agent-loops/summaries", get(handle_loop_summaries))
        .route("/agent-loops/stats", get(handle_loop_statistics))
        .route(
            "/agent-loops/{id}/status",
            patch(handle_update_loop_status).get(handle_loop_status),
        )
        .route("/agent-loops/{id}/run", post(handle_run_loop))
        .route("/agent-loops/{id}/stream", post(handle_stream_loop))
        .route("/agent-loops/{id}/pause", post(handle_pause_loop))
        .route("/agent-loops/{id}/resume", post(handle_resume_loop))
        .route("/agent-loops/{id}/cancel", post(handle_cancel_loop))
        .route(
            "/agent-loops/{id}/status/transition",
            post(handle_loop_status_transition),
        )
        .route(
            "/agent-loops/cleanup-completed",
            post(handle_cleanup_completed),
        )
        .route("/agent-loops/{id}/summary", get(handle_loop_summary))
        .route(
            "/agent-loops/{id}/iteration-history",
            get(handle_iteration_history),
        )
        .route(
            "/agent-loops/{id}/iteration-history/summary",
            get(handle_iteration_history_summary),
        )
        .route("/agent-loops/{id}/timeline", get(handle_loop_timeline))
        .route(
            "/agent-loops/{id}/variable-history/{name}",
            get(handle_variable_history),
        )
        .route(
            "/agent-loops/{id}/context-evolution",
            get(handle_loop_context_evolution),
        )
        .route(
            "/agent-loops/{id}/execution-path",
            get(handle_loop_execution_path),
        )
}

#[cfg(test)]
mod tests;

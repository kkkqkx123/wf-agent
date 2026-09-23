//! Template query surfaces: agent trigger / agent template query,
//! summaries, featured and popular. Split from `api_templates` to keep the
//! template surface at a maintainable file size.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::extract::ListQuery;
use crate::paged::{fetch_size, ok_page, resolve_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/templates/agent-trigger",
            get(handle_query_agent_trigger_templates),
        )
        .route(
            "/templates/agent-trigger/summaries",
            get(handle_agent_trigger_summaries),
        )
        .route("/templates/agent", get(handle_query_agent_templates))
        .route(
            "/templates/agent/summaries",
            get(handle_agent_template_summaries),
        )
        .route(
            "/templates/agent/featured",
            get(handle_agent_template_featured),
        )
        .route(
            "/templates/agent/popular",
            get(handle_agent_template_popular),
        )
}

async fn handle_query_agent_trigger_templates(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::trigger::template::query(&state.ctx, None).await {
        Ok(templates) => {
            let (limit, offset) = resolve_page(&query);
            let window = templates
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_agent_trigger_summaries(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::trigger::template::summaries(&state.ctx, None).await {
        Ok(summaries) => {
            let (limit, offset) = resolve_page(&query);
            let window = summaries
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_query_agent_templates(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::template::agent_template::query(&state.ctx, None) {
        Ok(templates) => {
            let (limit, offset) = resolve_page(&query);
            let window = templates
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_agent_template_summaries(
    State(state): State<ApiState>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::template::agent_template::summaries(&state.ctx, None) {
        Ok(summaries) => {
            let (limit, offset) = resolve_page(&query);
            let window = summaries
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

async fn handle_agent_template_featured(
    State(state): State<ApiState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    // Curated top-N catalog with caller-supplied cap; retained as a bare
    // array with no pagination.
    match wf_api::template::agent_template::featured(&state.ctx, query.limit) {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct CategoryLimitQuery {
    category: Option<String>,
    limit: Option<usize>,
}

async fn handle_agent_template_popular(
    State(state): State<ApiState>,
    Query(query): Query<CategoryLimitQuery>,
) -> impl IntoResponse {
    let result = match query.category {
        // Curated top-N catalog with caller-supplied cap; retained as a bare
        // array with no pagination.
        Some(category) => wf_api::template::agent_template::popular_in_category(
            &state.ctx,
            &category,
            query.limit,
        ),
        None => wf_api::template::agent_template::featured(&state.ctx, query.limit),
    };
    match result {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

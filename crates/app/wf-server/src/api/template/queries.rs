//! Template query surfaces: agent trigger / agent template query,
//! summaries, featured and popular. Split from `api_templates` to keep the
//! template surface at a maintainable file size.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::envelope::{error_response, ok};
use crate::paged::{fetch_size, ok_page, resolve_page_fields};
use crate::router::ApiState;

fn split_tags(raw: Option<&str>) -> Option<Vec<String>> {
    raw.map(|tags| {
        tags.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    })
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct AgentTriggerListQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    trigger_type: Option<String>,
    name: Option<String>,
    category: Option<String>,
    /// Comma-separated tags; a template matches when it carries any of them.
    tags: Option<String>,
    enabled: Option<bool>,
}

impl AgentTriggerListQuery {
    fn filter(&self) -> wf_api::AgentTriggerTemplateFilter {
        wf_api::AgentTriggerTemplateFilter {
            trigger_type: self.trigger_type.clone(),
            category: self.category.clone(),
            tags: split_tags(self.tags.as_deref()),
            enabled: self.enabled,
            name: self.name.clone(),
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct AgentTemplateListQuery {
    /// Page limit
    limit: Option<u64>,
    /// Page offset
    offset: Option<u64>,
    name: Option<String>,
    category: Option<String>,
    /// Comma-separated tags; a template matches when it carries any of them.
    tags: Option<String>,
    author: Option<String>,
    profile_type: Option<String>,
}

impl AgentTemplateListQuery {
    fn filter(&self) -> wf_api::AgentTemplateFilter {
        wf_api::AgentTemplateFilter {
            name: self.name.clone(),
            category: self.category.clone(),
            tags: split_tags(self.tags.as_deref()),
            author: self.author.clone(),
            profile_type: self.profile_type.clone(),
        }
    }
}

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

#[utoipa::path(
    get,
    path = "/api/v1/templates/agent-trigger",
    tag = "template",
    params(AgentTriggerListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_query_agent_trigger_templates(
    State(state): State<ApiState>,
    Query(query): Query<AgentTriggerListQuery>,
) -> impl IntoResponse {
    let filter = query.filter();
    match wf_api::trigger::template::query(&state.ctx, Some(&filter)).await {
        Ok(templates) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
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

#[utoipa::path(
    get,
    path = "/api/v1/templates/agent-trigger/summaries",
    tag = "template",
    params(AgentTriggerListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_trigger_summaries(
    State(state): State<ApiState>,
    Query(query): Query<AgentTriggerListQuery>,
) -> impl IntoResponse {
    let filter = query.filter();
    match wf_api::trigger::template::summaries(&state.ctx, Some(&filter)).await {
        Ok(summaries) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
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

#[utoipa::path(
    get,
    path = "/api/v1/templates/agent",
    tag = "template",
    params(AgentTemplateListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_query_agent_templates(
    State(state): State<ApiState>,
    Query(query): Query<AgentTemplateListQuery>,
) -> impl IntoResponse {
    let filter = query.filter();
    match wf_api::template::agent_template::query(&state.ctx, Some(&filter)) {
        Ok(templates) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
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

#[utoipa::path(
    get,
    path = "/api/v1/templates/agent/summaries",
    tag = "template",
    params(AgentTemplateListQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<crate::paged::PageView<serde_json::Value>>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_template_summaries(
    State(state): State<ApiState>,
    Query(query): Query<AgentTemplateListQuery>,
) -> impl IntoResponse {
    let filter = query.filter();
    match wf_api::template::agent_template::summaries(&state.ctx, Some(&filter)).await {
        Ok(summaries) => {
            let (limit, offset) = resolve_page_fields(query.limit, query.offset);
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/agent/featured",
    tag = "template",
    params(LimitQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_template_featured(
    State(state): State<ApiState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    // Curated top-N catalog with caller-supplied cap; retained as a bare
    // array with no pagination.
    match wf_api::template::agent_template::featured(&state.ctx, query.limit).await {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct CategoryLimitQuery {
    category: Option<String>,
    limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = "/api/v1/templates/agent/popular",
    tag = "template",
    params(CategoryLimitQuery),
    responses((status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("api_key" = []))
)]
pub(crate) async fn handle_agent_template_popular(
    State(state): State<ApiState>,
    Query(query): Query<CategoryLimitQuery>,
) -> impl IntoResponse {
    let result = match query.category {
        // Curated top-N catalog with caller-supplied cap; retained as a bare
        // array with no pagination.
        Some(category) => {
            wf_api::template::agent_template::popular_in_category(
                &state.ctx,
                &category,
                query.limit,
            )
            .await
        }
        None => wf_api::template::agent_template::featured(&state.ctx, query.limit).await,
    };
    match result {
        Ok(templates) => ok(templates).into_response(),
        Err(e) => error_response(e),
    }
}

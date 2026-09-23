//! Audit domain: execution audit queries (summary / report / timeline and
//! the iteration / tool-call / llm-call / node-execution facets). Handlers
//! are thin transport adapters over the `wf-api::audit` surface; unknown
//! executions map to NotFound through the shared envelope.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::extract::{IdPath, ListQuery};
use crate::paged::{fetch_size, ok_page, resolve_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/executions/{id}/audit/summary", get(handle_audit_summary))
        .route("/executions/{id}/audit/report", get(handle_audit_report))
        .route(
            "/executions/{id}/audit/timeline",
            get(handle_audit_timeline),
        )
        .route(
            "/executions/{id}/audit/iterations",
            get(handle_audit_iterations),
        )
        .route(
            "/executions/{id}/audit/tool-calls",
            get(handle_audit_tool_calls),
        )
        .route(
            "/executions/{id}/audit/llm-calls",
            get(handle_audit_llm_calls),
        )
        .route(
            "/executions/{id}/audit/node-executions",
            get(handle_audit_node_executions),
        )
}

async fn handle_audit_summary(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::audit::audit_summary(&state.ctx, &path.id).await {
        Ok(summary) => ok(summary).into_response(),
        Err(e) => error_response(e),
    }
}

async fn handle_audit_report(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<AuditReportQuery>,
) -> impl IntoResponse {
    match wf_api::audit::audit_report(&state.ctx, &path.id).await {
        Ok(report) => {
            if query.download.unwrap_or(false) {
                let payload = serde_json::to_string_pretty(&report).unwrap_or_default();
                crate::envelope::download(
                    &payload,
                    "application/json",
                    &format!("execution-{}-audit.json", path.id),
                )
                .into_response()
            } else {
                ok(report).into_response()
            }
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct AuditReportQuery {
    download: Option<bool>,
}

async fn handle_audit_timeline(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::audit::audit_timeline(&state.ctx, &path.id).await {
        Ok(timeline) => {
            let (limit, offset) = resolve_page(&query);
            let window = timeline
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_audit_iterations(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::audit::list_iterations(&state.ctx, &path.id).await {
        Ok(iterations) => {
            let (limit, offset) = resolve_page(&query);
            let window = iterations
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_audit_tool_calls(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::audit::list_tool_calls(&state.ctx, &path.id).await {
        Ok(calls) => {
            let (limit, offset) = resolve_page(&query);
            let window = calls
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_audit_llm_calls(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::audit::list_llm_calls(&state.ctx, &path.id).await {
        Ok(calls) => {
            let (limit, offset) = resolve_page(&query);
            let window = calls
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn handle_audit_node_executions(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    match wf_api::audit::list_node_executions(&state.ctx, &path.id).await {
        Ok(nodes) => {
            let (limit, offset) = resolve_page(&query);
            let window = nodes
                .into_iter()
                .skip(offset as usize)
                .take(fetch_size(limit) as usize)
                .collect();
            ok_page(window, limit, offset).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use axum::response::Response;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn audit_routes_degrade_to_empty_for_unknown_executions() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/executions/exec-1/audit/summary",
            "/api/v1/executions/exec-1/audit/report",
            "/api/v1/executions/exec-1/audit/timeline",
            "/api/v1/executions/exec-1/audit/iterations",
            "/api/v1/executions/exec-1/audit/tool-calls",
            "/api/v1/executions/exec-1/audit/llm-calls",
            "/api/v1/executions/exec-1/audit/node-executions",
        ] {
            // Read queries degrade to empty structures for unknown
            // executions instead of failing.
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
    }
}

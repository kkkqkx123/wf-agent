//! Dependency impact surface: reverse index over shared resources,
//! update impact reports and the stale (Expired lifecycle) set. Thin
//! transport adapters over `wf-api::infra::dependency` and the
//! `ApiContext` stale markers.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/dependencies/dependents", get(handle_dependents))
        .route("/dependencies/impact", get(handle_impact))
        .route("/dependencies/audit", get(handle_audit))
        .route("/system/stale", get(handle_stale))
}

#[derive(Deserialize)]
pub(crate) struct DependencyQuery {
    kind: String,
    id: String,
}

fn parse_kind(raw: &str) -> Option<wf_api::DependencyKind> {
    match raw.to_ascii_lowercase().as_str() {
        "tool" | "tools" => Some(wf_api::DependencyKind::Tool),
        "script" | "scripts" => Some(wf_api::DependencyKind::Script),
        "profile" | "profiles" => Some(wf_api::DependencyKind::Profile),
        "subworkflow" | "sub_workflow" | "sub-workflow" | "subworkflows" => {
            Some(wf_api::DependencyKind::SubWorkflow)
        }
        "trigger" | "triggers" => Some(wf_api::DependencyKind::Trigger),
        _ => None,
    }
}

#[utoipa::path(
    get,
    path = "/dependencies/dependents",
    tag = "system",
    params(("kind" = String, Query, description = "kind"), ("id" = String, Query, description = "id")),
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_dependents(
    State(state): State<ApiState>,
    Query(query): Query<DependencyQuery>,
) -> impl IntoResponse {
    let Some(kind) = parse_kind(&query.kind) else {
        return error_response(wf_api::ApiError::Validation(format!(
            "unknown dependency kind '{}': expected tool|script|profile|sub_workflow|trigger",
            query.kind
        )));
    };
    match wf_api::find_dependents(&state.ctx, kind, &query.id).await {
        Ok(dependents) => ok(dependents).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/dependencies/impact",
    tag = "system",
    params(("kind" = String, Query, description = "kind"), ("id" = String, Query, description = "id")),
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_impact(
    State(state): State<ApiState>,
    Query(query): Query<DependencyQuery>,
) -> impl IntoResponse {
    let Some(kind) = parse_kind(&query.kind) else {
        return error_response(wf_api::ApiError::Validation(format!(
            "unknown dependency kind '{}': expected tool|script|profile|sub_workflow|trigger",
            query.kind
        )));
    };
    match wf_api::check_update_impact(&state.ctx, kind, &query.id).await {
        Ok(report) => ok(report).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/dependencies/audit",
    tag = "system",
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_audit(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::audit_all_workflows(&state.ctx).await {
        Ok(items) => ok(items).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/system/stale",
    tag = "system",
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_stale(State(state): State<ApiState>) -> impl IntoResponse {
    ok(state.ctx.list_stale()).into_response()
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

    async fn get(ctx: Arc<ApiContext>, uri: &str) -> StatusCode {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn dependency_endpoints_answer_on_empty_state() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/dependencies/dependents?kind=tool&id=t-1",
            "/api/v1/dependencies/impact?kind=profile&id=p-1",
            "/api/v1/dependencies/audit",
            "/api/v1/system/stale",
        ] {
            assert_eq!(get(ctx.clone(), uri).await, StatusCode::OK, "uri: {uri}");
        }
        assert_eq!(
            get(ctx, "/api/v1/dependencies/impact?kind=bogus&id=x").await,
            StatusCode::BAD_REQUEST
        );
    }
}

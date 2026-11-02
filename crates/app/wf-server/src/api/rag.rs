//! Retrieval management routes: collection stats, previews and diagnosis.
//!
//! Read-only troubleshooting over the shared retrieval backend. When the
//! host did not inject retrieval handles, the routes report service
//! unavailable instead of failing silently.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct RagScope {
    #[param(example = "kb")]
    collection: String,
    #[param(example = "kb")]
    namespace: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct RagPreviewQuery {
    #[param(example = "kb")]
    collection: String,
    #[param(example = "kb")]
    namespace: String,
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
    #[serde(default = "default_excerpt")]
    excerpt_chars: usize,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct RagDiagnoseQuery {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: usize,
    #[serde(default)]
    filter_fields: usize,
    score_threshold: Option<f32>,
    #[param(example = "kb")]
    collection: Option<String>,
    #[param(example = "kb")]
    namespace: Option<String>,
}

fn default_top_k() -> usize {
    5
}

fn default_excerpt() -> usize {
    200
}

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/rag/stats", get(handle_rag_stats))
        .route("/rag/preview", get(handle_rag_preview))
        .route("/rag/diagnose", get(handle_rag_diagnose))
}

#[utoipa::path(
    get,
    path = "/api/v1/rag/stats",
    tag = "rag",
    params(RagScope),
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rag_stats(
    State(state): State<ApiState>,
    Query(scope): Query<RagScope>,
) -> impl IntoResponse {
    let handles = match wf_api::rag::require_rag_handles(state.ctx.rag_handles()) {
        Ok(handles) => handles,
        Err(e) => return error_response(e),
    };
    match wf_api::rag::collection_stats(
        handles.store.as_ref(),
        &scope.collection,
        &scope.namespace,
    )
    .await
    {
        Ok(stats) => ok(stats).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/rag/preview",
    tag = "rag",
    params(RagPreviewQuery),
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rag_preview(
    State(state): State<ApiState>,
    Query(query): Query<RagPreviewQuery>,
) -> impl IntoResponse {
    let handles = match wf_api::rag::require_rag_handles(state.ctx.rag_handles()) {
        Ok(handles) => handles,
        Err(e) => return error_response(e),
    };
    match wf_api::rag::preview_search(
        handles.store.as_ref(),
        handles.embedder.as_ref(),
        &query.collection,
        &query.namespace,
        &query.query,
        query.top_k,
        query.excerpt_chars,
    )
    .await
    {
        Ok(hits) => ok(hits).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/rag/diagnose",
    tag = "rag",
    params(RagDiagnoseQuery),
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<serde_json::Value>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rag_diagnose(
    State(state): State<ApiState>,
    Query(query): Query<RagDiagnoseQuery>,
) -> impl IntoResponse {
    let mut diagnosis = wf_api::rag::diagnose_no_hits(
        &query.query,
        query.top_k,
        query.filter_fields,
        query.score_threshold,
    );
    if let (Some(collection), Some(namespace)) = (query.collection, query.namespace) {
        if let Some(handles) = state.ctx.rag_handles() {
            if let Ok(stats) = wf_api::rag::collection_stats(
                handles.store.as_ref(),
                &collection,
                &namespace,
            )
            .await
            {
                diagnosis.likely_causes.push(format!(
                    "namespace '{namespace}' in collection '{collection}' holds {} points",
                    stats.point_count
                ));
            }
        }
    }
    ok(diagnosis).into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::Request;
    use axum::response::Response;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    fn ctx_without_rag() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ))
    }

    async fn get(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn rag_routes_report_unavailable_without_handles() {
        let ctx = ctx_without_rag();
        let response = get(
            ctx,
            "/api/v1/rag/stats?collection=kb&namespace=kb",
        )
        .await;
        assert_eq!(response.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
        let body = json_body(response).await;
        assert_eq!(body["success"], false);
    }

    #[tokio::test]
    async fn rag_diagnose_works_without_handles() {
        let ctx = ctx_without_rag();
        let response = get(ctx, "/api/v1/rag/diagnose?query=hello&top_k=5").await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["success"], true);
        assert!(body["data"]["likely_causes"].is_array());
    }
}

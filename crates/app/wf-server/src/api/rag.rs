//! Retrieval management routes: collection stats, previews and diagnosis.
//!
//! Read-only troubleshooting over the shared retrieval backend. When the
//! host did not inject retrieval handles, the routes report service
//! unavailable instead of failing silently.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

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

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RagSourceWrite {
    source: String,
    uri: String,
    kind: String,
    collection: String,
    namespace: String,
    #[serde(default)]
    converter: Option<String>,
    #[serde(default)]
    options: serde_json::Value,
    #[serde(default)]
    force: bool,
}

/// Identifies one indexed source. Used to delete it and to rebuild it, so a
/// rebuild never has to resend the reader kind, URI or reader options the
/// server already persisted with the source state.
#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RagSourceRef {
    source: String,
    collection: String,
    namespace: String,
}

fn default_top_k() -> usize {
    5
}

fn default_excerpt() -> usize {
    200
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct RagCollectionStatsView {
    pub collection: String,
    pub namespace: String,
    pub point_count: u64,
    pub keyword_point_count: Option<u64>,
}

impl From<wf_api::rag::CollectionStats> for RagCollectionStatsView {
    fn from(stats: wf_api::rag::CollectionStats) -> Self {
        Self {
            collection: stats.collection,
            namespace: stats.namespace,
            point_count: stats.point_count,
            keyword_point_count: stats.keyword_point_count,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct RagPreviewHitView {
    pub chunk_id: String,
    pub source: String,
    pub excerpt: String,
    pub score: f32,
}

impl From<wf_api::rag::PreviewHit> for RagPreviewHitView {
    fn from(hit: wf_api::rag::PreviewHit) -> Self {
        Self {
            chunk_id: hit.chunk_id,
            source: hit.source,
            excerpt: hit.excerpt,
            score: hit.score,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct RagNoHitDiagnosisView {
    pub likely_causes: Vec<String>,
}

impl From<wf_api::rag::NoHitDiagnosis> for RagNoHitDiagnosisView {
    fn from(diagnosis: wf_api::rag::NoHitDiagnosis) -> Self {
        Self {
            likely_causes: diagnosis.likely_causes,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct RagSourceRevisionView {
    pub content_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
}

impl From<source_contracts::SourceRevision> for RagSourceRevisionView {
    fn from(revision: source_contracts::SourceRevision) -> Self {
        Self {
            content_hash: revision.content_hash,
            etag: revision.etag,
            last_modified: revision.last_modified,
            fetched_at: revision.fetched_at,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RagSourceStateStatusView {
    Active,
    Failed,
}

impl From<wf_api::rag_source::RagSourceStatus> for RagSourceStateStatusView {
    fn from(status: wf_api::rag_source::RagSourceStatus) -> Self {
        match status {
            wf_api::rag_source::RagSourceStatus::Active => Self::Active,
            wf_api::rag_source::RagSourceStatus::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct RagSourceStateView {
    pub collection: String,
    pub namespace: String,
    pub source: String,
    pub kind: String,
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub converter: Option<String>,
    pub vector_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_schema: Option<String>,
    pub content_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    pub documents: usize,
    pub chunks: usize,
    pub points: usize,
    pub status: RagSourceStateStatusView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl From<wf_api::rag_source::RagSourceState> for RagSourceStateView {
    fn from(state: wf_api::rag_source::RagSourceState) -> Self {
        Self {
            collection: state.collection,
            namespace: state.namespace,
            source: state.source,
            kind: state.kind,
            uri: state.uri,
            converter: state.converter,
            vector_kind: state.vector_kind,
            encoder: state.encoder,
            encoder_fingerprint: state.encoder_fingerprint,
            encoder_schema: state.encoder_schema,
            content_hash: state.content_hash,
            etag: state.etag,
            last_modified: state.last_modified,
            fetched_at: state.fetched_at,
            documents: state.documents,
            chunks: state.chunks,
            points: state.points,
            status: state.status.into(),
            error: state.error,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct RagSourceStatusEntry {
    pub state: RagSourceStateView,
    pub point_count: u64,
    pub keyword_point_count: Option<u64>,
}

impl From<wf_api::rag_source::RagSourceStatusView> for RagSourceStatusEntry {
    fn from(view: wf_api::rag_source::RagSourceStatusView) -> Self {
        Self {
            state: view.state.into(),
            point_count: view.point_count,
            keyword_point_count: view.keyword_point_count,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RagSyncActionView {
    Insert,
    Update,
    Skip,
}

impl From<wf_api::rag_source::RagSourceSyncAction> for RagSyncActionView {
    fn from(action: wf_api::rag_source::RagSourceSyncAction) -> Self {
        match action {
            wf_api::rag_source::RagSourceSyncAction::Insert => Self::Insert,
            wf_api::rag_source::RagSourceSyncAction::Update => Self::Update,
            wf_api::rag_source::RagSourceSyncAction::Skip => Self::Skip,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct RagIngestResultView {
    pub source: String,
    pub action: RagSyncActionView,
    pub documents: usize,
    pub chunks: usize,
    pub points: usize,
    pub skipped: usize,
    pub content_hash: String,
    pub revision: RagSourceRevisionView,
    pub metadata: HashMap<String, String>,
}

impl From<wf_api::rag_source::RagSourceIngestResult> for RagIngestResultView {
    fn from(result: wf_api::rag_source::RagSourceIngestResult) -> Self {
        Self {
            source: result.source,
            action: result.action.into(),
            documents: result.documents,
            chunks: result.chunks,
            points: result.points,
            skipped: result.skipped,
            content_hash: result.content_hash,
            revision: result.revision.into(),
            metadata: result.metadata,
        }
    }
}

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/rag/stats", get(handle_rag_stats))
        .route("/rag/preview", get(handle_rag_preview))
        .route("/rag/diagnose", get(handle_rag_diagnose))
        .route(
            "/rag/sources",
            get(handle_rag_sources)
                .post(handle_rag_ingest_source)
                .delete(handle_rag_delete_source),
        )
        .route("/rag/sources/reindex", post(handle_rag_reindex_source))
}

#[utoipa::path(
    get,
    path = "/api/v1/rag/stats",
    tag = "rag",
    params(RagScope),
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<RagCollectionStatsView>),
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
    let keyword = state.ctx.effective_keyword_index();
    match wf_api::rag::collection_stats(
        handles.store.as_ref(),
        keyword.as_deref(),
        &scope.collection,
        &scope.namespace,
    )
    .await
    {
        Ok(stats) => ok(RagCollectionStatsView::from(stats)).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/rag/preview",
    tag = "rag",
    params(RagPreviewQuery),
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<Vec<RagPreviewHitView>>),
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
    let defaults = state
        .ctx
        .rag_defaults()
        .map(|defaults| defaults.query.clone())
        .unwrap_or_default();
    let keyword = state.ctx.effective_keyword_index();
    match wf_api::rag::preview_search(
        handles.store.as_ref(),
        handles.embedder.as_ref(),
        keyword.as_deref(),
        &defaults,
        &handles.extensions,
        wf_api::rag::PreviewParams {
            collection: &query.collection,
            namespace: &query.namespace,
            query: &query.query,
            top_k: query.top_k,
            excerpt_chars: query.excerpt_chars,
        },
    )
    .await
    {
        Ok(hits) => ok(hits
            .into_iter()
            .map(RagPreviewHitView::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/rag/diagnose",
    tag = "rag",
    params(RagDiagnoseQuery),
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<RagNoHitDiagnosisView>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rag_diagnose(
    State(state): State<ApiState>,
    Query(query): Query<RagDiagnoseQuery>,
) -> impl IntoResponse {
    let keyword_enabled = state.ctx.effective_keyword_index().is_some();
    let mut diagnosis = wf_api::rag::diagnose_no_hits(
        &query.query,
        query.top_k,
        query.filter_fields,
        query.score_threshold,
        keyword_enabled,
    );
    if let (Some(collection), Some(namespace)) = (query.collection, query.namespace) {
        if let Some(handles) = state.ctx.rag_handles() {
            let keyword = state.ctx.effective_keyword_index();
            if let Ok(stats) = wf_api::rag::collection_stats(
                handles.store.as_ref(),
                keyword.as_deref(),
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
    ok(RagNoHitDiagnosisView::from(diagnosis)).into_response()
}

fn source_service_error() -> wf_api::infra::error::ApiError {
    wf_api::infra::error::ApiError::execution_categorized(
        "RAG source service is not configured",
        wf_api::infra::error::ApiErrorCategory::ServiceUnavailable,
    )
}

fn descriptor_from_body(body: RagSourceWrite) -> (source_contracts::SourceDescriptor, bool) {
    let mut descriptor = source_contracts::SourceDescriptor::new(
        body.kind,
        body.source,
        body.uri,
        body.collection,
        body.namespace,
    );
    descriptor.converter = body.converter;
    if body.options.is_null() {
        descriptor.options = serde_json::json!({});
    } else {
        descriptor.options = body.options;
    }
    (descriptor, body.force)
}

#[utoipa::path(
    get,
    path = "/api/v1/rag/sources",
    tag = "rag",
    params(RagScope),
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<Vec<RagSourceStatusEntry>>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rag_sources(
    State(state): State<ApiState>,
    Query(scope): Query<RagScope>,
) -> impl IntoResponse {
    let Some(service) = state.ctx.rag_source_service().cloned() else {
        return error_response(source_service_error());
    };
    match service
        .status(&scope.collection, &scope.namespace, None)
        .await
    {
        Ok(views) => ok(views
            .into_iter()
            .map(RagSourceStatusEntry::from)
            .collect::<Vec<_>>())
        .into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/rag/sources",
    tag = "rag",
    request_body = RagSourceWrite,
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<RagIngestResultView>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rag_ingest_source(
    State(state): State<ApiState>,
    Json(body): Json<RagSourceWrite>,
) -> impl IntoResponse {
    let Some(service) = state.ctx.rag_source_service().cloned() else {
        return error_response(source_service_error());
    };
    let (descriptor, force) = descriptor_from_body(body);
    match service.ingest(descriptor, force).await {
        Ok(result) => ok(RagIngestResultView::from(result)).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/rag/sources/reindex",
    tag = "rag",
    request_body = RagSourceRef,
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<RagIngestResultView>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rag_reindex_source(
    State(state): State<ApiState>,
    Json(body): Json<RagSourceRef>,
) -> impl IntoResponse {
    let Some(service) = state.ctx.rag_source_service().cloned() else {
        return error_response(source_service_error());
    };
    match service
        .reindex_known(&body.collection, &body.namespace, &body.source)
        .await
    {
        Ok(result) => ok(RagIngestResultView::from(result)).into_response(),
        Err(err) => error_response(err),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/rag/sources",
    tag = "rag",
    request_body = RagSourceRef,
    responses(
        (status = 200, description = "Success", body = crate::envelope::ApiEnvelope<u64>),
        (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse),
    ),
    security(("api_key" = []))
)]
pub(crate) async fn handle_rag_delete_source(
    State(state): State<ApiState>,
    Json(body): Json<RagSourceRef>,
) -> impl IntoResponse {
    let Some(service) = state.ctx.rag_source_service().cloned() else {
        return error_response(source_service_error());
    };
    match service
        .delete(&body.collection, &body.namespace, &body.source)
        .await
    {
        Ok(removed) => ok(removed).into_response(),
        Err(err) => error_response(err),
    }
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
        let response = get(ctx, "/api/v1/rag/stats?collection=kb&namespace=kb").await;
        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
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

    struct UnitEmbedder {
        dimension: usize,
    }

    #[async_trait::async_trait]
    impl llm_embedding::provider::EmbeddingProvider for UnitEmbedder {
        async fn embed(
            &self,
            texts: &[String],
        ) -> llm_embedding::error::Result<llm_embedding::provider::EmbeddingResult> {
            let embeddings = texts.iter().map(|text| self.vector_for(text)).collect();
            Ok(llm_embedding::provider::EmbeddingResult {
                embeddings,
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn model_name(&self) -> &str {
            "test-unit"
        }
    }

    impl UnitEmbedder {
        fn vector_for(&self, text: &str) -> Vec<f32> {
            let mut vector = vec![0.0; self.dimension];
            for (index, byte) in text.bytes().enumerate() {
                vector[index % self.dimension] += f32::from(byte) / 255.0;
            }
            let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for value in &mut vector {
                    *value /= norm;
                }
            }
            vector
        }
    }

    #[tokio::test]
    async fn rag_stats_falls_back_to_context_keyword_index() {
        let store = std::sync::Arc::new(vector_contracts::InMemoryVectorStore::new());
        let embedder = std::sync::Arc::new(UnitEmbedder { dimension: 8 });
        let keyword: std::sync::Arc<dyn keyword_contracts::KeywordIndex> = std::sync::Arc::new(
            keyword_contracts::SqliteKeywordIndex::open_in_memory().expect("keyword"),
        );
        wf_rag::pipeline::ingest_documents(
            store.as_ref(),
            Some(keyword.as_ref()),
            embedder.as_ref(),
            "kb",
            &[wf_rag::RagDocument::new(
                "d1",
                "rust ownership borrowing rules",
                "rust.md",
            )],
            &wf_rag::RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        let ctx = std::sync::Arc::new(
            ApiContext::new(
                wf_storage::context::StorageContext::new_memory(),
                std::sync::Arc::new(wf_resource::registry::ResourceRegistries::new()),
            )
            .with_rag_handles(wf_api::rag::RagHandles::new(
                store,
                embedder,
                None,
                std::sync::Arc::new(wf_rag::RetrievalExtensions::new()),
            ))
            .with_keyword_index(keyword),
        );
        let response = get(ctx, "/api/v1/rag/stats?collection=kb&namespace=kb").await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["success"], true);
        let point_count = body["data"]["point_count"].as_u64().expect("count");
        assert!(point_count > 0);
        assert_eq!(body["data"]["keyword_point_count"], point_count);
    }
}

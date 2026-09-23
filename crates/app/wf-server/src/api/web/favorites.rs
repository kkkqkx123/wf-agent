//! Favorites (stars, pins, tags) over arbitrary resources, paged through
//! the shared `PageView` envelope.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::envelope::{error_response, ok};
use crate::extract::ListQuery;
use crate::paged::{ok_page, resolve_page};
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/favorites", get(handle_list_favorites))
        .route(
            "/favorites/{kind}/{id}",
            axum::routing::put(handle_upsert_favorite).delete(handle_delete_favorite),
        )
}

#[derive(Deserialize)]
pub(crate) struct ListFavoritesQuery {
    #[serde(flatten)]
    page: ListQuery,
    kind: Option<String>,
    pinned_only: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/favorites",
    tag = "web",
    params(("limit" = Option<u64>, Query, description = "limit"), ("offset" = Option<u64>, Query, description = "offset"), ("kind" = Option<String>, Query, description = "kind"), ("pinned_only" = Option<bool>, Query, description = "pinned_only")),
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_list_favorites(
    State(state): State<ApiState>,
    Query(query): Query<ListFavoritesQuery>,
) -> impl IntoResponse {
    let (limit, offset) = resolve_page(&query.page);
    // Window rule needs `limit + 1` items; favorites are few, so over-fetch
    // one page past the limit instead of a second query.
    let fetch_limit = limit.saturating_add(1).clamp(1, 501);
    match wf_api::web::favorites::list(
        &state.ctx,
        query.kind.as_deref(),
        query.pinned_only.unwrap_or(false),
        fetch_limit,
        offset,
    )
    .await
    {
        Ok((window, _)) => ok_page(window, limit, offset).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct FavoriteKindIdPath {
    kind: String,
    id: String,
}

#[derive(Deserialize)]
pub(crate) struct UpsertFavoriteBody {
    pinned: Option<bool>,
    tags: Option<Vec<String>>,
}

#[utoipa::path(
    put,
    path = "/favorites/{kind}/{id}",
    tag = "web",
    params(("kind" = String, Path, description = "kind"), ("id" = String, Path, description = "id")),
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_upsert_favorite(
    State(state): State<ApiState>,
    Path(path): Path<FavoriteKindIdPath>,
    Json(body): Json<UpsertFavoriteBody>,
) -> impl IntoResponse {
    match wf_api::web::favorites::upsert(&state.ctx, &path.kind, &path.id, body.pinned, body.tags)
        .await
    {
        Ok(entry) => ok(entry).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/favorites/{kind}/{id}",
    tag = "web",
    params(("kind" = String, Path, description = "kind"), ("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_delete_favorite(
    State(state): State<ApiState>,
    Path(path): Path<FavoriteKindIdPath>,
) -> impl IntoResponse {
    match wf_api::web::favorites::remove(&state.ctx, &path.kind, &path.id).await {
        Ok(removed) => ok(removed).into_response(),
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
        ))
    }

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn favorite_roundtrip_is_paged() {
        let ctx = make_ctx();
        let put = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/favorites/execution/e1")
                    .header("content-type", "application/json")
                    .body(AxBody::from(r#"{"pinned":true,"tags":["a"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::OK);

        let listed = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/favorites")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = json_body(listed).await;
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["has_more"], false);
    }
}

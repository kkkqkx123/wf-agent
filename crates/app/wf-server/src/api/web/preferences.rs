//! UI preference document: whole-document and single-key access over the
//! `wf-api::web::preferences` surface. The server validates the envelope
//! only; preference semantics stay in the frontend.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::envelope::{error_response, ok};
use crate::extract::IdPath;
use crate::router::ApiState;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/preferences",
            get(handle_get_preferences).put(handle_replace_preferences),
        )
        .route(
            "/preferences/{id}",
            get(handle_get_preference)
                .put(handle_set_preference)
                .delete(handle_delete_preference),
        )
}

#[utoipa::path(
    get,
    path = "/preferences",
    tag = "web",
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_get_preferences(State(state): State<ApiState>) -> impl IntoResponse {
    match wf_api::web::preferences::get_all(&state.ctx).await {
        Ok(values) => ok(values).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct ReplacePreferencesBody {
    #[serde(default)]
    values: Map<String, Value>,
}

#[utoipa::path(
    put,
    path = "/preferences",
    tag = "web",
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_replace_preferences(
    State(state): State<ApiState>,
    Json(body): Json<ReplacePreferencesBody>,
) -> impl IntoResponse {
    match wf_api::web::preferences::replace_all(&state.ctx, body.values).await {
        Ok(values) => ok(values).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/preferences/{id}",
    tag = "web",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_get_preference(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::web::preferences::get_key(&state.ctx, &path.id).await {
        Ok(Some(value)) => ok(value).into_response(),
        Ok(None) => error_response(wf_api::ApiError::not_found("preference", &path.id)),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct SetPreferenceBody {
    value: Value,
}

#[utoipa::path(
    put,
    path = "/preferences/{id}",
    tag = "web",
    params(("id" = String, Path, description = "id")),
    request_body = serde_json::Value,
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 400, description = "Invalid parameters", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_set_preference(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
    Json(body): Json<SetPreferenceBody>,
) -> impl IntoResponse {
    match wf_api::web::preferences::set_key(&state.ctx, &path.id, body.value).await {
        Ok(value) => ok(value).into_response(),
        Err(e) => error_response(e),
    }
}

#[utoipa::path(
    delete,
    path = "/preferences/{id}",
    tag = "web",
    params(("id" = String, Path, description = "id")),
    responses((status = 200, description = "Success", body = serde_json::Value), (status = 404, description = "Not found", body = crate::envelope::ErrorResponse), (status = 500, description = "Internal server error", body = crate::envelope::ErrorResponse)),
    security(("bearer_auth" = []))
)]
pub(crate) async fn handle_delete_preference(
    State(state): State<ApiState>,
    Path(path): Path<IdPath>,
) -> impl IntoResponse {
    match wf_api::web::preferences::delete_key(&state.ctx, &path.id).await {
        Ok(deleted) => ok(deleted).into_response(),
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
    async fn preference_key_roundtrip() {
        let ctx = make_ctx();
        let put = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/preferences/theme")
                    .header("content-type", "application/json")
                    .body(AxBody::from(r#"{"value":"dark"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::OK);

        let get = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/preferences/theme")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = json_body(get).await;
        assert_eq!(body["data"], "dark");

        let missing = crate::router::api_router(ctx.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/preferences/nope")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let deleted = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/preferences/theme")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = json_body(deleted).await;
        assert_eq!(body["data"], true);
    }
}

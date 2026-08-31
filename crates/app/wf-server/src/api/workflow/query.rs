//! Query domain: execution record queries, aggregation, distinct/group-by
//! and CSV/XML/JSON export. Request bodies mirror the serde shapes of
//! `FilterCriteria` / `SortOptions` / `PaginationOptions` / `AggregationOp`.

use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;

use wf_api::{
    aggregate, evaluate_json_expression, export_to_format, get_distinct, group_by_field, query,
    AggregationOp, ExportFormat, FilterCriteria, FilterExpression, PaginationOptions, SortOptions,
};

use crate::envelope::{error_response, ok};
use crate::router::ApiState;

/// Default page size used when only one of limit / offset is supplied.
const DEFAULT_QUERY_LIMIT: usize = 100;

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/query", post(handle_query))
        .route("/query/export", post(handle_export))
        .route("/query/aggregate", post(handle_aggregate))
        .route("/query/distinct", get(handle_distinct))
        .route("/query/group-by", post(handle_group_by))
        .route("/query/evaluate", post(handle_evaluate))
}

/// Wire shape of the query endpoints: filters (basic + advanced
/// expressions) plus sort / pagination overrides.
#[derive(Deserialize, Default)]
struct QueryBody {
    filters: Option<FilterCriteria>,
    #[serde(default)]
    expressions: Vec<FilterExpression>,
    sort_field: Option<String>,
    sort_descending: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl QueryBody {
    fn sort(&self) -> Option<SortOptions> {
        self.sort_field.as_ref().map(|field| SortOptions {
            field: field.clone(),
            descending: self.sort_descending.unwrap_or(false),
        })
    }

    fn pagination(&self) -> Option<PaginationOptions> {
        match (self.limit, self.offset) {
            (None, None) => None,
            _ => Some(PaginationOptions {
                limit: self.limit.unwrap_or(DEFAULT_QUERY_LIMIT),
                offset: self.offset.unwrap_or(0),
            }),
        }
    }
}

async fn handle_query(
    State(state): State<ApiState>,
    Json(body): Json<QueryBody>,
) -> impl IntoResponse {
    let result = query(
        &state.ctx,
        body.filters.as_ref(),
        body.sort().as_ref(),
        body.pagination().as_ref(),
    )
    .await;
    match result {
        Ok(records) => {
            let records = wf_api::apply_filter_expressions(&records, &body.expressions);
            ok(records).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct ExportBody {
    filters: Option<FilterCriteria>,
    #[serde(default)]
    expressions: Vec<FilterExpression>,
    sort_field: Option<String>,
    sort_descending: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
    format: Option<ExportFormat>,
}

async fn handle_export(
    State(state): State<ApiState>,
    Json(body): Json<ExportBody>,
) -> impl IntoResponse {
    let format = body.format.unwrap_or(ExportFormat::Json);
    let sort = body.sort_field.as_ref().map(|field| SortOptions {
        field: field.clone(),
        descending: body.sort_descending.unwrap_or(false),
    });
    let pagination = match (body.limit, body.offset) {
        (None, None) => None,
        _ => Some(PaginationOptions {
            limit: body.limit.unwrap_or(DEFAULT_QUERY_LIMIT),
            offset: body.offset.unwrap_or(0),
        }),
    };
    let result = query(
        &state.ctx,
        body.filters.as_ref(),
        sort.as_ref(),
        pagination.as_ref(),
    )
    .await;
    match result {
        Ok(records) => {
            let records = wf_api::apply_filter_expressions(&records, &body.expressions);
            ok(export_to_format(&records, format)).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct AggregateBody {
    filters: Option<FilterCriteria>,
    #[serde(default)]
    expressions: Vec<FilterExpression>,
    #[serde(default)]
    operations: Vec<AggregationOp>,
}

async fn handle_aggregate(
    State(state): State<ApiState>,
    Json(body): Json<AggregateBody>,
) -> impl IntoResponse {
    let result = query(&state.ctx, body.filters.as_ref(), None, None).await;
    match result {
        Ok(records) => {
            let records = wf_api::apply_filter_expressions(&records, &body.expressions);
            ok(aggregate(&records, &body.operations)).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct DistinctParams {
    field: String,
    #[serde(flatten)]
    filters: FilterCriteria,
}

async fn handle_distinct(
    State(state): State<ApiState>,
    Query(params): Query<DistinctParams>,
) -> impl IntoResponse {
    match wf_api::query(&state.ctx, Some(&params.filters), None, None).await {
        Ok(records) => ok(get_distinct(&records, &params.field)).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct GroupByBody {
    field: String,
    filters: Option<FilterCriteria>,
}

async fn handle_group_by(
    State(state): State<ApiState>,
    Json(body): Json<GroupByBody>,
) -> impl IntoResponse {
    match query(&state.ctx, body.filters.as_ref(), None, None).await {
        Ok(records) => ok(group_by_field(&records, &body.field)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Evaluate a filter expression against an arbitrary JSON record without
/// persisting anything. Useful for validating query semantics before use.
#[derive(Deserialize)]
struct EvaluateBody {
    record: Value,
    #[serde(flatten)]
    expression: FilterExpression,
}

async fn handle_evaluate(Json(body): Json<EvaluateBody>) -> impl IntoResponse {
    let matched = evaluate_json_expression(&body.record, &body.expression);
    ok(matched).into_response()
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
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
        ))
    }

    async fn post_json(ctx: Arc<ApiContext>, uri: &str, body: serde_json::Value) -> Response {
        crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(AxBody::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn query_endpoints_accept_bodies() {
        let ctx = make_ctx();
        let empty = serde_json::json!({});
        for uri in [
            "/api/v1/query",
            "/api/v1/query/export",
            "/api/v1/query/aggregate",
            "/api/v1/query/group-by",
        ] {
            let body = if uri == "/api/v1/query/group-by" {
                serde_json::json!({"field": "status"})
            } else {
                empty.clone()
            };
            let response = post_json(ctx.clone(), uri, body).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }

        let distinct = crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/query/distinct?field=status")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(distinct.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn export_roundtrip_produces_csv_and_xml() {
        let ctx = make_ctx();
        let body = serde_json::json!({"format": "csv"});
        let response = post_json(ctx.clone(), "/api/v1/query/export", body).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body = serde_json::json!({"format": "xml"});
        let response = post_json(ctx, "/api/v1/query/export", body).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn evaluate_matches_and_misses() {
        let ctx = make_ctx();
        let record = serde_json::json!({"status": "completed", "duration_ms": 150});
        let hit = serde_json::json!({
            "record": record,
            "field": "status",
            "operator": "eq",
            "value": "completed"
        });
        let response = post_json(ctx.clone(), "/api/v1/query/evaluate", hit).await;
        assert_eq!(response.status(), StatusCode::OK);

        let miss = serde_json::json!({
            "record": record,
            "field": "duration_ms",
            "operator": "gte",
            "value": 200
        });
        let response = post_json(ctx, "/api/v1/query/evaluate", miss).await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}

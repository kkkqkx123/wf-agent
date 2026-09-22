//! Machine-readable HTTP contract: `GET /api/v1/contract`.
//!
//! The route inventory is generated from the `.route()` declarations (see the
//! generator note below) so frontend codegen never drifts from the server.
//! The probe test requests every listed route and fails on drift: a listed
//! route that 404s with an empty body (axum fallback) means the declaration
//! moved, and the generator keeps the file in sync.
//!
//! Regenerate with `scripts/gen_server_contract.py` (plus the `/contract`
//! route itself, appended by hand since it lives outside `src/api`).

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::router::ApiState;

/// Generated inventory; kept next to this module so `include_str!` stays put.
const CONTRACT_JSON: &str = include_str!("contract.json");

pub(crate) fn routes() -> Router<ApiState> {
    Router::new().route("/contract", get(handle_contract))
}

async fn handle_contract() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        CONTRACT_JSON,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body as AxBody;
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;
    use wf_api::ApiContext;

    use super::CONTRACT_JSON;

    fn probe_path(path: &str) -> String {
        let mut out = String::with_capacity(path.len());
        let mut rest = path;
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            match rest[start..].find('}') {
                Some(end) => {
                    out.push_str("probe");
                    rest = &rest[start + end + 1..];
                }
                None => {
                    out.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn probe_paths_substitute_params() {
        assert_eq!(
            probe_path("/api/v1/agent-loops/{id}/checkpoints/{cid}/resume"),
            "/api/v1/agent-loops/probe/checkpoints/probe/resume"
        );
        assert_eq!(probe_path("/health"), "/health");
    }

    #[tokio::test]
    async fn contract_routes_all_resolve() {
        let ctx = Arc::new(ApiContext::new(
            wf_storage::context::StorageContext::new_memory(),
            Arc::new(wf_resource::registry::ResourceRegistries::new()),
        ));
        let doc: Value = serde_json::from_str(CONTRACT_JSON).unwrap();
        let routes = doc["routes"].as_array().unwrap();
        assert!(
            routes.len() > 300,
            "contract must cover the API surface, got {}",
            routes.len()
        );
        let mut failures = Vec::new();
        for route in routes {
            if !route["probe"].as_bool().unwrap_or(true) {
                continue;
            }
            let method = route["method"].as_str().unwrap();
            let path = probe_path(route["path"].as_str().unwrap());
            let builder = Request::builder().method(method).uri(&path);
            let request = if matches!(method, "POST" | "PUT" | "PATCH") {
                builder
                    .header("content-type", "application/json")
                    .body(AxBody::from("{}"))
                    .unwrap()
            } else {
                builder.body(AxBody::empty()).unwrap()
            };
            let response = crate::router::api_router(ctx.clone())
                .oneshot(request)
                .await
                .unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap();
            // Undocumented drift shows as axum's empty-body 404; every real
            // route answers with the JSON envelope (even for missing entities)
            // or a rejection body.
            if status == StatusCode::NOT_FOUND && bytes.is_empty() {
                failures.push(format!("{method} {path}"));
            }
        }
        assert!(
            failures.is_empty(),
            "contract drift, routes not resolving: {failures:?}"
        );
    }
}

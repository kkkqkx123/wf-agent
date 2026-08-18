//! Agent domain composition: profiles, loops, executions, checkpoints,
//! messages, variables, triggers and interactions. Each surface lives in a
//! sibling module; this file only merges them and hosts the shared tests.

use axum::Router;

use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .merge(crate::api::agent::profiles::routes())
        .merge(crate::api::agent::loops::routes())
        .merge(crate::api::agent::executions::routes())
        .merge(crate::api::agent::variables::routes())
        .merge(crate::api::agent::triggers::routes())
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

    async fn send(ctx: Arc<ApiContext>, method: &str, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn agent_executions_are_queryable() {
        let ctx = make_ctx();
        let agent_executions = send(ctx.clone(), "GET", "/api/v1/agent-executions").await;
        assert_eq!(agent_executions.status(), StatusCode::OK);

        for uri in [
            "/api/v1/agent-executions/stats",
            "/api/v1/agent-executions/by-status/running",
            "/api/v1/agent-executions/by-status/completed",
            "/api/v1/agent-executions/by-definition/def-1",
            "/api/v1/agents",
            "/api/v1/agent-loops",
            "/api/v1/agent-triggers/stats",
            "/api/v1/agent-checkpoints/stats",
        ] {
            let response = send(ctx.clone(), "GET", uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }

        let unknown_status = send(ctx, "GET", "/api/v1/agent-executions/by-status/bogus").await;
        assert_eq!(unknown_status.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn agent_loop_control_maps_not_found() {
        let ctx = make_ctx();
        // Control endpoints require a live entity and map to NotFound.
        for (method, uri) in [
            ("POST", "/api/v1/agent-loops/missing/pause"),
            ("POST", "/api/v1/agent-loops/missing/resume"),
            ("POST", "/api/v1/agent-loops/missing/cancel"),
            ("GET", "/api/v1/agent-loops/missing/status"),
        ] {
            let response = send(ctx.clone(), method, uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
        // Read views degrade to empty results for unknown loops.
        for (method, uri) in [
            ("GET", "/api/v1/agent-loops/missing/summary"),
            ("GET", "/api/v1/agent-loops/missing/timeline"),
            ("GET", "/api/v1/agent-loops/missing/messages"),
            ("GET", "/api/v1/agent-loops/missing/variables"),
            ("GET", "/api/v1/agent-loops/missing/interactions"),
        ] {
            let response = send(ctx.clone(), method, uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
    }
}

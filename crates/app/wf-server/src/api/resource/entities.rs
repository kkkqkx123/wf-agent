//! Entity resource domain composition: tasks, triggers, trigger executions,
//! variables, messages and skills. Each surface lives in a sibling module;
//! this file only merges them and hosts the shared tests.

use axum::Router;

use crate::router::ApiState;
pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .merge(crate::api::workflow::tasks::routes())
        .merge(crate::api::resource::triggers::routes())
        .merge(crate::api::resource::variables::routes())
        .merge(crate::api::workflow::messages::routes())
        .merge(crate::api::agent::skills::routes())
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

    async fn send(ctx: Arc<ApiContext>, uri: &str) -> Response {
        crate::router::api_router(ctx)
            .oneshot(Request::builder().uri(uri).body(AxBody::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn entity_endpoints_are_reachable() {
        let ctx = make_ctx();
        for uri in [
            "/api/v1/tasks",
            "/api/v1/tasks/stats",
            "/api/v1/tasks/by-execution/exec-1",
            "/api/v1/triggers",
            "/api/v1/triggers/stats",
            "/api/v1/triggers/search?q=push",
            "/api/v1/trigger-executions",
            "/api/v1/trigger-executions/stats",
            "/api/v1/trigger-executions/by-execution/exec-1",
            "/api/v1/trigger-executions/by-trigger/push",
            "/api/v1/trigger-executions/by-workflow/wf-1",
            "/api/v1/variables",
            "/api/v1/variables/stats",
            "/api/v1/variables/history?name=foo",
            "/api/v1/variables/export/exec-1",
            "/api/v1/variables/scopes/exec-1",
            "/api/v1/variables/scope/default",
            "/api/v1/variables/by-node/exec-1/node-1",
            "/api/v1/messages",
            "/api/v1/messages/stats",
            "/api/v1/messages/search?q=hello",
            "/api/v1/messages/by-execution/exec-1",
            "/api/v1/messages/conversation/exec-1",
            "/api/v1/skills",
            "/api/v1/skills/query",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
        // Single-resource routes map unknown ids to NotFound.
        for uri in [
            "/api/v1/tasks/missing",
            "/api/v1/triggers/missing",
            "/api/v1/trigger-executions/missing",
            "/api/v1/variables/missing?scope=default",
            "/api/v1/messages/missing",
        ] {
            let response = send(ctx.clone(), uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri: {uri}");
        }
        // Skills require a configured skill loader; the default context has
        // none, so every skill read surfaces as an error response.
        let skills = send(ctx, "/api/v1/skills/missing").await;
        assert_eq!(skills.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn skill_resources_rejects_unknown_type() {
        let ctx = make_ctx();
        let response = send(ctx, "/api/v1/skills/foo/resources?resource_type=bogus").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn variable_batch_and_trigger_cleanup_accept_bodies() {
        let ctx = make_ctx();
        let router = crate::router::api_router(ctx);

        for (method, uri, body) in [
            (
                "POST",
                "/api/v1/variables/batch",
                r#"{"execution_id":"e-1","entries":[{"name":"a","scope":"default","value":1}]}"#,
            ),
            (
                "POST",
                "/api/v1/variables/import",
                r#"{"execution_id":"e-1","values":{"a":1}}"#,
            ),
            (
                "POST",
                "/api/v1/trigger-executions/cleanup",
                r#"{"older_than":1}"#,
            ),
            ("POST", "/api/v1/trigger-executions/cleanup", r#"{}"#),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .body(AxBody::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "uri: {uri}");
        }
    }
}

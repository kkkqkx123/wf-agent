//! Router composition for the application-facing `wf-api` surface: every
//! domain module (under `crates/wf-server/src/api/{workflow,agent,resource}`)
//! contributes its routes over an `Arc<wf_api::ApiContext>`;
//! `api_router_with_config` merges them into one router and
//! `serve_api_with_config` binds it to a TCP listener with graceful shutdown.
//! Metrics endpoints (`crates/wf-server/src/metrics.rs`) can be merged through
//! `full_router_with_middleware` / `serve_full_with_config`.

use std::sync::Arc;

use axum::Router;

use wf_api::ApiContext;

use crate::middleware::{self, ServerMiddlewareConfig};
use crate::server::{serve_with_router, ServeError, ServerHandle};
use crate::server_config::ServerConfig;
use crate::{api, metrics, ws};

#[derive(Clone)]
pub(crate) struct ApiState {
    pub(crate) ctx: Arc<ApiContext>,
    pub(crate) config: Arc<ServerMiddlewareConfig>,
}

/// Test-only API router with deterministic default middleware (no environment
/// reads, so one process env cannot flake another test). Production and
/// embedding paths use `api_router_with_config` / `serve_*_with_config`.
#[cfg(test)]
pub(crate) fn api_router(ctx: Arc<ApiContext>) -> Router {
    api_router_with_config(ctx, Arc::new(ServerMiddlewareConfig::default()))
}

/// Build the `wf-api` router (execution / query / event stream / websocket)
/// with a programmable middleware configuration. Domain routes are mounted
/// under `/api/v1`; the system surface (`/health`, `/system/*`, `/`) stays at
/// the root. Embedding and tests use this to exercise auth / rate limiting /
/// CORS on the full API surface.
pub fn api_router_with_config(ctx: Arc<ApiContext>, config: Arc<ServerMiddlewareConfig>) -> Router {
    use api::agent::{agents, analysis as agent_analysis, llm};
    use api::resource::{entities, health, openapi, templates};
    use api::workflow::{analysis, approvals, audit, events, executions, hooks, query, workflows};

    let domain: Router<ApiState> = Router::new()
        .merge(workflows::routes())
        .merge(executions::routes())
        .merge(hooks::routes())
        .merge(approvals::routes())
        .merge(crate::api::workflow::file_approvals::routes())
        .merge(crate::api::workflow::file_provenance::routes())
        .merge(audit::routes())
        .merge(agents::routes())
        .merge(agent_analysis::routes())
        .merge(llm::routes())
        .merge(templates::routes())
        .merge(entities::routes())
        .merge(query::routes())
        .merge(analysis::routes())
        .merge(events::routes())
        .merge(openapi::routes())
        .merge(ws::routes());
    let app = Router::new()
        .merge(health::routes())
        .nest("/api/v1", domain);
    let app = middleware::apply(app, Arc::clone(&config));
    app.with_state(ApiState { ctx, config })
}

/// Metrics + API router merged under one surface with a programmable
/// middleware configuration.
pub fn full_router_with_middleware(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    config: Arc<ServerMiddlewareConfig>,
) -> Router {
    let http = registry.http();
    let router = metrics::router(registry).merge(api_router_with_config(ctx, config));
    middleware::with_request_metrics(router, Some(http))
}

/// Serve the `wf-api` surface on the address owned by `config`, applying the
/// middleware owned by `config` (file layer plus environment).
pub async fn serve_api_with_config(
    ctx: Arc<ApiContext>,
    config: &ServerConfig,
) -> Result<ServerHandle, ServeError> {
    let router = api_router_with_config(ctx, Arc::new(config.middleware.clone()));
    serve_with_router(router, config.bind_addr).await
}

/// Serve metrics + API on the address owned by `config`, applying the
/// middleware owned by `config` (file layer plus environment).
pub async fn serve_full_with_config(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    config: &ServerConfig,
) -> Result<ServerHandle, ServeError> {
    let router = full_router_with_middleware(registry, ctx, Arc::new(config.middleware.clone()));
    serve_with_router(router, config.bind_addr).await
}

/// Test helper: metrics + API router with a programmable middleware
/// configuration.
#[cfg(test)]
pub(crate) async fn serve_full_with_middleware(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    addr: std::net::SocketAddr,
    config: Arc<ServerMiddlewareConfig>,
) -> Result<ServerHandle, ServeError> {
    let router = full_router_with_middleware(registry, ctx, config);
    serve_with_router(router, addr).await
}

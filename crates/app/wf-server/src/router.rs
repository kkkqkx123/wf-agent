//! Router composition for the application-facing `wf-api` surface: every
//! domain module (under `crates/wf-server/src/api/{workflow,agent,resource}`)
//! contributes its routes over an `Arc<wf_api::ApiContext>`; `api_router`
//! merges them into one router and `serve_api` binds it to a TCP listener
//! with graceful shutdown. Metrics endpoints (`crates/wf-server/src/metrics.rs`)
//! can be merged through `full_router` / `serve_full`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;

use wf_api::ApiContext;

use crate::middleware::{self, ServerMiddlewareConfig};
use crate::server::{serve_with_router, ServeError, ServerHandle};
use crate::{api, metrics, ws};

#[derive(Clone)]
pub(crate) struct ApiState {
    pub(crate) ctx: Arc<ApiContext>,
    pub(crate) config: Arc<ServerMiddlewareConfig>,
}

/// Build the `wf-api` router (execution / query / event stream / websocket).
/// Domain routes are mounted under `/api/v1`; system surface (`/health`,
/// `/system/*`, `/`) stays at the root.
pub fn api_router(ctx: Arc<ApiContext>) -> Router {
    api_router_with_config(ctx, middleware::default_config())
}

/// `api_router` with a programmable middleware configuration (tests use this
/// to exercise auth / rate limiting / CORS on the full API surface).
pub(crate) fn api_router_with_config(
    ctx: Arc<ApiContext>,
    config: Arc<ServerMiddlewareConfig>,
) -> Router {
    use api::agent::{agents, analysis as agent_analysis, llm};
    use api::resource::{entities, health, openapi, templates};
    use api::workflow::{analysis, approvals, audit, events, executions, query, workflows};

    let domain: Router<ApiState> = Router::new()
        .merge(workflows::routes())
        .merge(executions::routes())
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

/// Merge the metrics router and the API router under one listener.
pub fn full_router(registry: Arc<wf_metrics::MetricsRegistry>, ctx: Arc<ApiContext>) -> Router {
    metrics::router(registry).merge(api_router(ctx))
}

/// Serve the `wf-api` surface on `addr` without blocking.
pub async fn serve_api(ctx: Arc<ApiContext>, addr: SocketAddr) -> Result<ServerHandle, ServeError> {
    serve_with_router(api_router(ctx), addr).await
}

/// Serve metrics + API on the same listener.
pub async fn serve_full(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    addr: SocketAddr,
) -> Result<ServerHandle, ServeError> {
    serve_with_router(full_router(registry, ctx), addr).await
}

/// `serve_full` with a programmable middleware configuration (tests).
#[cfg(test)]
pub(crate) async fn serve_full_with_config(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    addr: SocketAddr,
    config: Arc<ServerMiddlewareConfig>,
) -> Result<ServerHandle, ServeError> {
    serve_with_router(
        metrics::router(registry).merge(api_router_with_config(ctx, config)),
        addr,
    )
    .await
}

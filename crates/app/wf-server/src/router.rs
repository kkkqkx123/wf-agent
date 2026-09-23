//! Router composition for the application-facing `wf-api` surface: every
//! domain module (under `crates/app/wf-server/src/api/{workflow,agent,
//! checkpoint,trigger,template,llm,entity,observation,system}`)
//! contributes its routes over an `Arc<wf_api::ApiContext>`;
//! `api_router_with_config` merges them into one router and
//! `serve_api_with_config` binds it to a TCP listener with graceful shutdown.
//! Metrics endpoints (`crates/app/wf-server/src/metrics.rs`) can be merged through
//! `full_router_with_middleware` / `serve_full_with_config`.
//!
//! Composition is intentionally single-layer: each leaf module owns only its
//! own `routes()` and this file is the sole merge point, so the module tree
//! mirrors the `wf-api` domain layout.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;

use wf_api::ApiContext;

use crate::middleware::{self, ServerMiddlewareConfig};
use crate::server::{serve_with_router, ServeError, ServerHandle};
use crate::server_config::ServerConfig;
use crate::static_files::serve_static;
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
    let domain: Router<ApiState> = Router::new()
        // workflow definition + execution + graph analysis
        .merge(api::workflow::workflows::routes())
        .merge(api::workflow::versions::routes())
        .merge(api::workflow::graphs::routes())
        .merge(api::workflow::executions::routes())
        .merge(api::workflow::execution_state::routes())
        .merge(api::workflow::execution_analysis::routes())
        .merge(api::workflow::approvals::routes())
        .merge(api::workflow::drafts::routes())
        // agent loop + profiles + decision surface
        .merge(api::agent::profiles::routes())
        .merge(api::agent::loops::routes())
        .merge(api::agent::executions::routes())
        .merge(api::agent::graphs::routes())
        .merge(api::agent::analysis::routes())
        .merge(api::agent::variables::routes())
        .merge(api::agent::drafts::routes())
        // shared checkpoint domain (execution records + file workspace)
        .merge(api::checkpoint::checkpoints::routes())
        .merge(api::checkpoint::file_provenance::routes())
        .merge(api::checkpoint::file_approvals::routes())
        // trigger domain (ledger + template activation gateway)
        .merge(api::trigger::executions::routes())
        .merge(api::trigger::hooks::routes())
        // web support domain (preferences + favorites + batch)
        .merge(api::web::preferences::routes())
        .merge(api::web::favorites::routes())
        .merge(api::web::batch::routes())
        // llm domain (generation + profiles + providers + scripts + tools)
        .merge(api::llm::llm::routes())
        .merge(api::llm::scripts::routes())
        .merge(api::llm::tools::routes())
        // template domain
        .merge(api::template::templates::routes())
        .merge(api::template::queries::routes())
        .merge(api::template::library::routes())
        // entity domain (low-level storage CRUD + interactions + skills)
        .merge(api::entity::messages::routes())
        .merge(api::entity::tasks::routes())
        .merge(api::entity::variables::routes())
        .merge(api::entity::skills::routes())
        .merge(api::entity::interactions::routes())
        // observation domain (query + audit + analysis over executions)
        .merge(api::observation::query::routes())
        .merge(api::observation::audit::routes())
        .merge(api::observation::analysis::routes())
        // system domain (events + impact + discovery; health carries its
        // own absolute prefixes and is merged at the root below)
        .merge(api::system::events::routes())
        .merge(api::system::dependencies::routes())
        .merge(ws::routes());
    let app = Router::new()
        .merge(api::system::health::routes())
        .nest("/api/v1", domain);
    let app = middleware::apply(app, Arc::clone(&config));
    let app = app.with_state(ApiState { ctx, config });
    mount_openapi_docs(app)
}

/// Mount the OpenAPI JSON document in dev/debug builds or with the
/// `openapi-docs` feature. Release builds without the feature expose no
/// `/api-docs/*` routes. Mounted after state erasure so the plain `Router`
/// is used. The offline snapshot under `apps/web-app` is the codegen source;
/// this route only serves a live server for debugging.
#[cfg(any(debug_assertions, feature = "openapi-docs"))]
fn mount_openapi_docs(router: Router) -> Router {
    use axum::routing::get;
    router.route(
        "/api-docs/openapi.json",
        get(crate::openapi::serve_openapi_json),
    )
}

/// Release builds without the feature expose no docs routes.
#[cfg(not(any(debug_assertions, feature = "openapi-docs")))]
fn mount_openapi_docs(router: Router) -> Router {
    router
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
    let router = with_static_fallback(router, &config.static_dir);
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
    let router = with_static_fallback(router, &config.static_dir);
    serve_with_router(router, config.bind_addr).await
}

/// Layer the static-file SPA fallback onto `router` when a frontend build
/// directory is configured; API-only mode leaves the router untouched.
fn with_static_fallback<S>(router: Router<S>, static_dir: &Option<PathBuf>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let Some(dir) = static_dir.clone() else {
        return router;
    };
    let dir = Arc::new(dir);
    router.fallback(move |uri: axum::http::Uri| {
        let dir = Arc::clone(&dir);
        async move { serve_static(&dir, uri.path()).await }
    })
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

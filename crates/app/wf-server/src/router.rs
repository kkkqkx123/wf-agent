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

use std::sync::Arc;

use axum::Router;

use wf_api::ApiContext;

use crate::directory::{directory_middleware, execution_existence_middleware, DirectoryContext};
#[cfg(feature = "metrics")]
use crate::metrics;
use crate::middleware::{self, ServerMiddlewareConfig};
use crate::server::{serve_with_router_and_backends, BackendHandle, ServeError, ServerHandle};
use crate::server_config::{ServerConfig, StaticConfig};
use crate::static_files::{serve_static, StaticHosting};
use crate::{api, ws};

#[derive(Clone)]
pub(crate) struct ApiState {
    pub(crate) ctx: Arc<ApiContext>,
    pub(crate) config: Arc<ServerMiddlewareConfig>,
    pub(crate) dirs: Arc<DirectoryContext>,
    pub(crate) locks: api::workflow::locks::DefinitionLocks,
    #[cfg(feature = "backends")]
    pub(crate) backends: BackendHandle,
}

/// Public API version reported by `/api/v1/info`. The constant lives in
/// `wf-types` so the server response and client expectation cannot drift.
pub use wf_types::API_VERSION;

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
/// CORS on the full API surface. Embedding stays API-only: the `/` root
/// keeps the JSON answer and no static fallback is layered.
pub fn api_router_with_config(ctx: Arc<ApiContext>, config: Arc<ServerMiddlewareConfig>) -> Router {
    let dirs = Arc::new(DirectoryContext::new(
        crate::directory::DirectoryConfig::default(),
    ));
    api_router_with_dirs(ctx, config, dirs)
}

/// Same as [`api_router_with_config`] with an explicit directory context
/// for multi-directory serving. Requests resolve their directory scope
/// from the `x-wf-directory` header or the `directory` query parameter;
/// there is no default root, so directory-less requests carry no scope
/// and workspace-bound handlers reject them. Rejections render through the
/// shared error envelope before reaching domain handlers.
pub fn api_router_with_dirs(
    ctx: Arc<ApiContext>,
    config: Arc<ServerMiddlewareConfig>,
    dirs: Arc<DirectoryContext>,
) -> Router {
    api_router_with_dirs_and_backends(ctx, config, dirs, None, false)
}

/// Same as [`api_router_with_dirs`] with an explicit backend manager for
/// the server-owned registry, tunnel and backup domain. `None` keeps the
/// backend surface disabled (503) so existing tests and direct-link
/// deployments observe zero behavior change. `static_enabled` hands the
/// `/` root to the app shell; API-only callers keep `false`.
pub fn api_router_with_dirs_and_backends(
    ctx: Arc<ApiContext>,
    config: Arc<ServerMiddlewareConfig>,
    dirs: Arc<DirectoryContext>,
    backends: BackendHandle,
    static_enabled: bool,
) -> Router {
    #[cfg(not(feature = "backends"))]
    let _ = &backends;
    let domain: Router<ApiState> = Router::new()
        // workflow definition + execution + graph analysis
        .merge(api::workflow::workflows::routes())
        .merge(api::workflow::locks::routes())
        .merge(api::workflow::versions::routes())
        .merge(api::workflow::graphs::routes())
        .merge(api::workflow::executions::routes())
        .merge(api::workflow::variable_injections::routes())
        .merge(api::workflow::execution_state::routes())
        .merge(api::workflow::execution_analysis::routes())
        .merge(api::workflow::approvals::routes())
        .merge(api::workflow::drafts::routes())
        // agent loop + profiles + decision surface
        .merge(api::agent::profiles::routes())
        .merge(api::agent::definitions::routes())
        .merge(api::agent::loops::routes())
        .merge(api::agent::executions::routes())
        .merge(api::agent::graphs::routes())
        .merge(api::agent::analysis::routes())
        .merge(api::agent::variables::routes())
        .merge(api::agent::drafts::routes())
        .merge(api::agent::locks::routes())
        // chat domain (tool-free multi-turn executor)
        .merge(api::chat::routes())
        // cli snapshot (data-only plugin CLI projection for shells)
        .merge(api::cli::routes())
        // screen-only recap reads (chat sessions + agent loops share the face)
        .merge(api::recap::routes())
        .merge(api::title::routes())
        // shared checkpoint domain (execution records + file workspace)
        .merge(api::checkpoint::checkpoints::routes());

    #[cfg(feature = "file-checkpoint")]
    let domain = domain
        .merge(api::checkpoint::file_provenance::routes())
        .merge(api::checkpoint::file_approvals::routes())
        .merge(api::checkpoint::file_sync::routes());

    // user git domain (read-only queries + unified action)
    #[cfg(feature = "git-fs")]
    let domain = domain
        .merge(api::git::readonly::routes())
        .merge(api::git::actions::routes())
        .merge(api::fs::routes());

    #[cfg(feature = "pty")]
    let domain = domain.merge(api::pty::routes());

    let domain = domain
        // trigger domain (ledger + template activation gateway)
        .merge(api::trigger::executions::routes())
        .merge(api::trigger::hooks::routes())
        // web support domain (preferences + favorites + batch)
        .merge(api::web::preferences::routes())
        .merge(api::web::favorites::routes())
        .merge(api::web::batch::routes())
        // llm domain (generation + profiles + providers + scripts + tools)
        .merge(api::llm::routes())
        .merge(api::llm::scripts::routes())
        .merge(api::llm::tools::routes());

    // retrieval management (stats + preview + diagnose)
    #[cfg(feature = "rag-vector")]
    let domain = domain.merge(api::rag::routes());

    let domain = domain
        // template domain
        .merge(api::template::templates::routes())
        .merge(api::template::node_drafts::routes())
        .merge(api::template::node_versions::routes())
        .merge(api::template::trigger_drafts::routes())
        .merge(api::template::trigger_versions::routes())
        .merge(api::template::locks::routes())
        // template library (unified discovery + instantiation)
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
        .merge(api::observation::hierarchy::routes())
        .merge(api::observation::history::routes())
        .merge(api::observation::listing::routes())
        .merge(api::observation::logs::routes())
        .merge(api::observation::artifacts::routes())
        .merge(api::observation::bundle::routes())
        .merge(api::recovery::routes())
        // system domain (events + impact + discovery; health carries its
        // own absolute prefixes and is merged at the root below)
        .merge(api::system::events::routes())
        .merge(api::system::dependencies::routes())
        .merge(api::system::code_context::routes())
        .merge(api::system::directories::routes())
        .merge(api::system::server::routes());

    #[cfg(feature = "backends")]
    let domain = domain.merge(api::system::backends::routes());

    let domain = domain.merge(ws::routes());
    // Static hosting owns the `/` root for the app shell; the JSON root
    // answer steps aside and discovery stays on `/api/v1/info`. Named
    // routes (`/health`, `/metrics`, `/system/*`) keep API precedence.
    let health = if static_enabled {
        api::system::health::routes_without_root()
    } else {
        api::system::health::routes()
    };
    let app = Router::new().merge(health).nest("/api/v1", domain);
    // Execution read views share one existence gate: unknown ids fail as
    // not-found here instead of degrading to empty results per handler.
    // Layered inside auth so authenticated checks keep their precedence.
    let app = app.layer(axum::middleware::from_fn_with_state(
        Arc::clone(&ctx),
        execution_existence_middleware,
    ));
    let app = middleware::apply(app, Arc::clone(&config));
    let app = app.layer(axum::middleware::from_fn_with_state(
        Arc::clone(&dirs),
        directory_middleware,
    ));
    let app = app.with_state(ApiState {
        ctx,
        config,
        dirs,
        locks: api::workflow::locks::DefinitionLocks::default(),
        #[cfg(feature = "backends")]
        backends,
    });
    mount_openapi_docs(app)
}

/// Mount the OpenAPI JSON document in dev/debug builds or with the
/// `openapi-docs` feature. Release builds without the feature expose no
/// `/api-docs/*` routes. Mounted after state erasure so the plain `Router`
/// is used. The offline snapshot under `apps/web-app` is the codegen source;
/// this route only serves a live server for debugging.
#[cfg(all(
    any(debug_assertions, feature = "openapi-docs"),
    feature = "file-checkpoint",
    feature = "git-fs",
    feature = "pty",
    feature = "rag-vector",
    feature = "backends",
    feature = "metrics"
))]
fn mount_openapi_docs(router: Router) -> Router {
    use axum::routing::get;
    router.route(
        "/api-docs/openapi.json",
        get(crate::openapi::serve_openapi_json),
    )
}

/// Release builds without the feature expose no docs routes.
#[cfg(not(all(
    any(debug_assertions, feature = "openapi-docs"),
    feature = "file-checkpoint",
    feature = "git-fs",
    feature = "pty",
    feature = "rag-vector",
    feature = "backends",
    feature = "metrics"
)))]
fn mount_openapi_docs(router: Router) -> Router {
    router
}

/// Metrics + API router merged under one surface with a programmable
/// middleware configuration.
pub fn full_router_with_middleware(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    config: Arc<ServerMiddlewareConfig>,
    dirs: Arc<DirectoryContext>,
) -> Router {
    full_router_with_middleware_and_backends(registry, ctx, config, dirs, None, false)
}

/// Same as [`full_router_with_middleware`] with an explicit backend
/// manager. Production servers pass the manager built from
/// `ServerConfig::backends`; tests keep `None` for the disabled surface.
/// `static_enabled` hands the `/` root to the app shell.
pub fn full_router_with_middleware_and_backends(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    config: Arc<ServerMiddlewareConfig>,
    dirs: Arc<DirectoryContext>,
    backends: BackendHandle,
    static_enabled: bool,
) -> Router {
    let api = api_router_with_dirs_and_backends(ctx, config, dirs, backends, static_enabled);
    #[cfg(feature = "metrics")]
    let router = {
        let http = registry.http();
        let router = metrics::router(registry).merge(api);
        middleware::with_request_metrics(router, Some(http))
    };
    #[cfg(not(feature = "metrics"))]
    let router = {
        let _ = registry;
        api
    };
    router
}

/// Serve the `wf-api` surface on the address owned by `config`, applying the
/// middleware owned by `config` (file layer plus environment).
pub async fn serve_api_with_config(
    ctx: Arc<ApiContext>,
    config: &ServerConfig,
    dirs: Arc<DirectoryContext>,
) -> Result<ServerHandle, ServeError> {
    let registry = Arc::new(wf_metrics::MetricsRegistry::new());
    let backends = backend_manager_for_config(&config.backends, registry).await?;
    #[cfg(feature = "backends")]
    if let Some(manager) = backends.as_ref() {
        manager.start_background();
    }
    #[cfg(not(feature = "backends"))]
    let _ = &backends;
    let router = api_router_with_dirs_and_backends(
        ctx,
        Arc::new(config.middleware.clone()),
        dirs,
        backends.clone(),
        config.static_config.is_some(),
    );
    let router = with_static_fallback(router, config.static_config.as_ref());
    serve_with_router_and_backends(router, config.bind_addr, backends).await
}

/// Resolve the backend manager from config. Both call sites use `?`, so the
/// function reports a failure instead of degrading.
#[cfg(feature = "backends")]
async fn backend_manager_for_config(
    config: &crate::backends::BackendConfig,
    registry: Arc<wf_metrics::MetricsRegistry>,
) -> Result<BackendHandle, ServeError> {
    use crate::backends::{BackendManager, BackendStore};
    if !config.enabled {
        return Ok(None);
    }
    // A present but unusable index must fail startup: a memory fallback would
    // keep writing backup files that no index and no pruning can see.
    let store = match config.registry_path.as_deref() {
        Some(path) => BackendStore::open_sqlite_file(path).await.map_err(|e| {
            ServeError::Server(format!(
                "cannot open backend registry {}: {e}",
                path.display()
            ))
        })?,
        None => match config.backup_dir.as_deref() {
            Some(dir) => {
                let path = dir.join("backends.db");
                BackendStore::open_sqlite_file(&path).await.map_err(|e| {
                    ServeError::Server(format!(
                        "cannot open backend registry {}: {e}",
                        path.display()
                    ))
                })?
            }
            None => BackendStore::new_memory(),
        },
    };
    Ok(Some(BackendManager::new_with_metrics(
        config.clone(),
        store,
        Some(registry),
    )))
}

#[cfg(not(feature = "backends"))]
async fn backend_manager_for_config(
    _config: &crate::backends::BackendConfig,
    _registry: Arc<wf_metrics::MetricsRegistry>,
) -> Result<BackendHandle, ServeError> {
    Ok(None)
}

/// Serve metrics + API on the address owned by `config`, applying the
/// middleware owned by `config` (file layer plus environment).
pub async fn serve_full_with_config(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    config: &ServerConfig,
    dirs: Arc<DirectoryContext>,
) -> Result<ServerHandle, ServeError> {
    let backends = backend_manager_for_config(&config.backends, Arc::clone(&registry)).await?;
    #[cfg(feature = "backends")]
    if let Some(manager) = backends.as_ref() {
        manager.start_background();
    }
    #[cfg(not(feature = "backends"))]
    let _ = &backends;
    let router = full_router_with_middleware_and_backends(
        registry,
        ctx,
        Arc::new(config.middleware.clone()),
        dirs,
        backends.clone(),
        config.static_config.is_some(),
    );
    let router = with_static_fallback(router, config.static_config.as_ref());
    serve_with_router_and_backends(router, config.bind_addr, backends).await
}

/// Layer the static-file SPA fallback onto `router` when frontend hosting
/// is configured; API-only mode leaves the router untouched. The fallback
/// sits outside auth, rate limiting and CORS: pages carry no project data,
/// so every route layer keeps gating only the API surface.
fn with_static_fallback<S>(router: Router<S>, static_config: Option<&StaticConfig>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let Some(config) = static_config else {
        return router;
    };
    let hosting = Arc::new(StaticHosting::from_config(config));
    router.fallback(
        move |method: axum::http::Method, headers: axum::http::HeaderMap, uri: axum::http::Uri| {
            let hosting = Arc::clone(&hosting);
            async move { serve_static(&hosting, &method, uri.path(), &headers).await }
        },
    )
}

/// Test helper: metrics + API router with a programmable middleware
/// configuration.
#[cfg(test)]
pub(crate) async fn serve_full_with_middleware(
    registry: Arc<wf_metrics::MetricsRegistry>,
    ctx: Arc<ApiContext>,
    addr: std::net::SocketAddr,
    config: Arc<ServerMiddlewareConfig>,
    dirs: Arc<DirectoryContext>,
) -> Result<ServerHandle, ServeError> {
    let router = full_router_with_middleware(registry, ctx, config, dirs);
    serve_with_router_and_backends(router, addr, None).await
}

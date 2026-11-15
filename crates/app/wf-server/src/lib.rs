pub mod api;
pub mod auth_session;
pub mod backends;
pub mod directory;
pub mod envelope;
pub mod extract;
pub mod metrics;
pub mod middleware;
pub mod openapi;
pub mod paged;
pub mod pairing;
pub mod router;
pub mod server;
pub mod server_config;
pub mod sse;
pub mod static_files;
pub mod ws;

pub use api::system::health::HealthView;
pub use api::workflow::executions::{ExecuteBody, ExecuteView};
pub use metrics::{router, serve};
pub use router::{
    api_router_with_config, api_router_with_dirs, api_router_with_dirs_and_backends,
    full_router_with_middleware, full_router_with_middleware_and_backends, serve_api_with_config,
    serve_full_with_config,
};
pub use server::{ServeError, ServerHandle};
pub use server_config::{
    ServerConfig, StaticConfig, DEFAULT_STATIC_IMMUTABLE_MAX_AGE_SECS, DEFAULT_STATIC_INDEX,
    STATIC_DIR_ENV_VAR, STATIC_FALLBACK_ENV_VAR, STATIC_IMMUTABLE_MAX_AGE_ENV_VAR,
    STATIC_INDEX_ENV_VAR,
};

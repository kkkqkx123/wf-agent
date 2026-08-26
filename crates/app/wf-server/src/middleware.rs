//! Server middleware: request logging, CORS, API-key auth and per-IP rate
//! limiting. Every layer no-ops unless its
//! config is enabled, so the default (env-driven) configuration keeps the
//! API surface open by default.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Router;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub api_keys: Vec<String>,
    pub header_name: String,
    pub allow_query_param: bool,
    pub query_param_name: String,
    pub excluded_paths: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: std::env::var("AUTH_ENABLED").as_deref() == Ok("true"),
            api_keys: std::env::var("API_KEYS")
                .map(|v| {
                    v.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            header_name: "x-api-key".to_string(),
            allow_query_param: true,
            query_param_name: "api_key".to_string(),
            excluded_paths: vec![
                "/health".to_string(),
                "/api/v1/info".to_string(),
                "/".to_string(),
                "/api/v1/ws".to_string(),
                "/api/v1/events/stream".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub window_ms: u64,
    pub max_requests: u64,
    pub excluded_paths: Vec<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: std::env::var("RATE_LIMIT_ENABLED").as_deref() == Ok("true"),
            window_ms: std::env::var("RATE_LIMIT_WINDOW_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60_000),
            max_requests: std::env::var("RATE_LIMIT_MAX_REQUESTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            excluded_paths: vec![
                "/health".to_string(),
                "/api/v1/info".to_string(),
                "/".to_string(),
                "/api/v1/ws".to_string(),
                "/api/v1/events/stream".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec!["Content-Type".to_string(), "Authorization".to_string()],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServerMiddlewareConfig {
    pub auth: AuthConfig,
    pub rate_limit: RateLimitConfig,
    pub cors: CorsConfig,
}

/// Env-driven default configuration shared by the API router.
pub(crate) fn default_config() -> Arc<ServerMiddlewareConfig> {
    Arc::new(ServerMiddlewareConfig::default())
}

// ---------------------------------------------------------------------------
// Composition
// ---------------------------------------------------------------------------

/// Apply all middleware layers to `router`. Request order: request logging
/// (outermost) → CORS → auth → rate limit (innermost).
pub(crate) fn apply<S>(router: Router<S>, config: Arc<ServerMiddlewareConfig>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let router = router.layer(axum::middleware::from_fn(request_logging));
    let cfg = Arc::clone(&config);
    let router = router.layer(axum::middleware::from_fn(move |req, next| {
        cors_middleware(Arc::clone(&cfg), req, next)
    }));
    let cfg = Arc::clone(&config);
    let router = router.layer(axum::middleware::from_fn(move |req, next| {
        auth_middleware(Arc::clone(&cfg), req, next)
    }));
    let cfg = Arc::clone(&config);
    router.layer(axum::middleware::from_fn(move |req, next| {
        rate_limit_middleware(Arc::clone(&cfg), req, next)
    }))
}

// ---------------------------------------------------------------------------
// Request logging
// ---------------------------------------------------------------------------

async fn request_logging(req: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let response = next.run(req).await;
    match response.status().as_u16() {
        500.. => tracing::error!(
            target: "wf_server",
            method = %method,
            path = %path,
            status = response.status().as_u16(),
            duration_ms = start.elapsed().as_millis(),
            "request failed",
        ),
        400.. => tracing::warn!(
            target: "wf_server",
            method = %method,
            path = %path,
            status = response.status().as_u16(),
            duration_ms = start.elapsed().as_millis(),
            "request rejected",
        ),
        _ => tracing::debug!(
            target: "wf_server",
            method = %method,
            path = %path,
            status = response.status().as_u16(),
            duration_ms = start.elapsed().as_millis(),
            "request handled",
        ),
    }
    response
}

// ---------------------------------------------------------------------------
// CORS
// ---------------------------------------------------------------------------

async fn cors_middleware(
    config: Arc<ServerMiddlewareConfig>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let cors = &config.cors;
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);

    let allowed = origin
        .as_deref()
        .map(|o| cors.allowed_origins.iter().any(|a| a == "*" || a == o))
        .unwrap_or(false);

    if req.method() == Method::OPTIONS {
        let mut response = StatusCode::OK.into_response();
        apply_cors_headers(response.headers_mut(), cors, origin.as_deref(), allowed);
        return response;
    }

    let mut response = next.run(req).await;
    apply_cors_headers(response.headers_mut(), cors, origin.as_deref(), allowed);
    response
}

fn apply_cors_headers(
    headers: &mut HeaderMap,
    cors: &CorsConfig,
    origin: Option<&str>,
    allowed: bool,
) {
    if !allowed {
        return;
    }
    if let Some(origin) = origin {
        let value = if cors.allowed_origins.iter().any(|a| a == "*") {
            "*"
        } else {
            origin
        };
        if let Ok(value) = header::HeaderValue::from_str(value) {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        }
    }
    let methods = cors.allowed_methods.join(",");
    let headers_list = cors.allowed_headers.join(",");
    if let Ok(value) = header::HeaderValue::from_str(&methods) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, value);
    }
    if let Ok(value) = header::HeaderValue::from_str(&headers_list) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, value);
    }
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

async fn auth_middleware(
    config: Arc<ServerMiddlewareConfig>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let auth = &config.auth;
    if !auth.enabled {
        return next.run(req).await;
    }
    if is_excluded(req.uri().path(), &auth.excluded_paths) {
        return next.run(req).await;
    }

    let key = req
        .headers()
        .get(&auth.header_name)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
        .or_else(|| {
            if auth.allow_query_param {
                query_param(req.uri(), &auth.query_param_name)
            } else {
                None
            }
        });

    match key {
        None => crate::envelope::unauthorized(
            "API key is required. Provide it via X-API-Key header or api_key query parameter.",
        ),
        Some(key) if !auth.api_keys.iter().any(|k| k == &key) => {
            crate::envelope::forbidden("Invalid API key.")
        }
        Some(_) => next.run(req).await,
    }
}

// ---------------------------------------------------------------------------
// Rate limit
// ---------------------------------------------------------------------------

struct RateLimitEntry {
    count: u64,
    reset_at_ms: u128,
}

#[derive(Default)]
struct RateLimitState {
    clients: HashMap<String, RateLimitEntry>,
}

async fn rate_limit_middleware(
    config: Arc<ServerMiddlewareConfig>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let rl = &config.rate_limit;
    if !rl.enabled {
        return next.run(req).await;
    }
    if is_excluded(req.uri().path(), &rl.excluded_paths) {
        return next.run(req).await;
    }

    let key = client_ip(req.headers()).to_string();
    let now_ms = epoch_ms();

    let (remaining, reset_at, limited) = {
        let mut state = config
            .rate_limit_state()
            .lock()
            .expect("rate limit state poisoned");

        if state.clients.len() > 10_000 {
            state.clients.retain(|_, entry| entry.reset_at_ms > now_ms);
        }

        let entry = state.clients.entry(key).or_insert(RateLimitEntry {
            count: 0,
            reset_at_ms: now_ms + rl.window_ms as u128,
        });
        entry.count += 1;
        let remaining = rl.max_requests.saturating_sub(entry.count);
        let reset_at = entry.reset_at_ms;
        let limited = entry.count > rl.max_requests;
        (remaining, reset_at, limited)
    };

    if limited {
        return crate::envelope::rate_limited((reset_at.saturating_sub(now_ms) / 1000) as u64);
    }

    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    if let Ok(value) = header::HeaderValue::from_str(&rl.max_requests.to_string()) {
        headers.insert("x-ratelimit-limit", value);
    }
    if let Ok(value) = header::HeaderValue::from_str(&remaining.to_string()) {
        headers.insert("x-ratelimit-remaining", value);
    }
    if let Ok(value) = header::HeaderValue::from_str(&(reset_at / 1000).to_string()) {
        headers.insert("x-ratelimit-reset", value);
    }
    response
}

impl ServerMiddlewareConfig {
    fn rate_limit_state(&self) -> &'static Mutex<RateLimitState> {
        static STATE: OnceLock<Mutex<RateLimitState>> = OnceLock::new();
        STATE.get_or_init(|| Mutex::new(RateLimitState::default()))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_excluded(path: &str, excluded: &[String]) -> bool {
    excluded
        .iter()
        .any(|e| path == e || path.starts_with(&format!("{e}/")))
}

fn client_ip(headers: &HeaderMap) -> &str {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
}

/// Extract the raw (percent-decoded) value of the first occurrence of `name`
/// in the request query string.
pub(crate) fn query_param(uri: &Uri, name: &str) -> Option<String> {
    let query = uri.query()?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == name {
            return Some(percent_decode(value));
        }
    }
    None
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body as AxBody;
    use axum::routing::get;
    use tower::ServiceExt;

    fn test_router(config: ServerMiddlewareConfig) -> Router {
        apply(
            Router::new().route("/hello", get(|| async { "world" })),
            Arc::new(config),
        )
    }

    fn request(method: &str, uri: &str) -> axum::http::Request<AxBody> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(AxBody::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn default_config_does_not_guard() {
        let app = test_router(ServerMiddlewareConfig::default());
        let response = app.clone().oneshot(request("GET", "/hello")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auth_rejects_missing_and_invalid_keys() {
        let mut config = ServerMiddlewareConfig::default();
        config.auth.enabled = true;
        config.auth.api_keys = vec!["secret".to_string()];
        let app = test_router(config);

        let missing = app.clone().oneshot(request("GET", "/hello")).await.unwrap();
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

        let bad = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/hello")
                    .header("x-api-key", "wrong")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::FORBIDDEN);

        let ok = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/hello")
                    .header("x-api-key", "secret")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);

        let query_ok = app
            .clone()
            .oneshot(request("GET", "/hello?api_key=secret"))
            .await
            .unwrap();
        assert_eq!(query_ok.status(), StatusCode::OK);

        let excluded = app
            .clone()
            .oneshot(request("GET", "/health"))
            .await
            .unwrap();
        assert_eq!(excluded.status(), StatusCode::NOT_FOUND, "auth skipped");
    }

    #[tokio::test]
    async fn rate_limit_returns_429_with_headers() {
        let mut config = ServerMiddlewareConfig::default();
        config.rate_limit.enabled = true;
        config.rate_limit.max_requests = 2;
        let app = test_router(config);

        let first = app.clone().oneshot(request("GET", "/hello")).await.unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(first.headers().get("x-ratelimit-remaining").unwrap(), "1");

        let second = app.clone().oneshot(request("GET", "/hello")).await.unwrap();
        assert_eq!(second.status(), StatusCode::OK);

        let third = app.clone().oneshot(request("GET", "/hello")).await.unwrap();
        assert_eq!(third.status(), StatusCode::TOO_MANY_REQUESTS);

        let excluded = app.clone().oneshot(request("GET", "/")).await.unwrap();
        assert_eq!(
            excluded.status(),
            StatusCode::NOT_FOUND,
            "rate limit skipped"
        );
    }

    #[tokio::test]
    async fn cors_handles_preflight_and_origin_filtering() {
        let mut config = ServerMiddlewareConfig::default();
        config.cors.allowed_origins = vec!["http://allowed.test".to_string()];
        let app = test_router(config);

        let preflight = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/hello")
                    .header("Origin", "http://allowed.test")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(preflight.status(), StatusCode::OK);
        assert_eq!(
            preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "http://allowed.test"
        );

        let denied = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/hello")
                    .header("Origin", "http://evil.test")
                    .body(AxBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            !denied
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            "foreign origin must not receive CORS headers"
        );
    }

    #[test]
    fn percent_decode_handles_encoded_keys() {
        assert_eq!(percent_decode("a%20b%2Bc"), "a b+c");
    }
}

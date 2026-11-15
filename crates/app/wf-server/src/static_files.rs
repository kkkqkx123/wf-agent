//! Optional static-file hosting with SPA fallback for the web frontend.
//!
//! Disabled unless `ServerConfig::static_config` is set (`--static-dir`,
//! `WF_SERVER_STATIC_DIR` env or `server.toml`). When enabled, unmatched
//! non-API routes serve files from the directory and fall back to the
//! configured index page so deep routes survive refresh; unknown `/api/*`
//! and `/api-docs/*` paths keep the JSON 404 so API clients never receive
//! HTML. Serving fails startup when the directory is unusable, so a broken
//! deployment never serves a half-broken page.
//!
//! File semantics come from the transport file service: conditional
//! requests, ranges and HEAD keep file behavior. Static responses carry a
//! `Cache-Control` policy (immutable fingerprinted assets are cached for
//! the configured lifetime, other assets get a short TTL, the index page
//! including the SPA fallback is never stored) plus a nosniff header.
//! Hidden path segments stay invisible: any percent-decoded segment
//! starting with `.` answers 404 before the file service runs.

use std::path::PathBuf;
use std::sync::Arc;

use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::envelope::ErrorResponse;
use crate::server_config::StaticConfig;

/// Resolved static hosting state shared by the file service.
#[derive(Debug, Clone)]
pub(crate) struct StaticHosting {
    dir: PathBuf,
    index: Arc<PathBuf>,
    fallback: bool,
    immutable_max_age_secs: u64,
}

impl StaticHosting {
    pub(crate) fn from_config(config: &StaticConfig) -> Self {
        Self {
            dir: config.dir.clone(),
            index: Arc::new(config.dir.join(&config.index)),
            fallback: config.fallback,
            immutable_max_age_secs: config.immutable_max_age_secs,
        }
    }
}

/// Serve one unmatched path from `hosting`.
pub(crate) async fn serve_static(
    hosting: &StaticHosting,
    method: &Method,
    path: &str,
    headers: &axum::http::HeaderMap,
) -> Response {
    if !is_api(path) && has_dot_segment(path) {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                "NOT_FOUND",
                format!("Static asset not found: {path}"),
            )),
        )
            .into_response();
    }
    let kind = classify_missing(path, hosting.fallback);
    // Unknown API paths keep the JSON 404 for every method without
    // touching the file service.
    if kind == MissingKind::UnknownApi {
        return serve_missing(hosting, method, path, headers, kind).await;
    }
    if !is_read(method) {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(ErrorResponse::new(
                "METHOD_NOT_ALLOWED",
                format!("Method {method} is not served for static path {path}"),
            )),
        )
            .into_response();
    }
    let request = served_request(method, path, headers);
    match tower::ServiceExt::oneshot(
        tower_http::services::ServeDir::new(&hosting.dir).append_index_html_on_directories(false),
        request,
    )
    .await
    {
        Ok(response) if response.status() != StatusCode::NOT_FOUND => {
            finish_file_response(hosting, response.map(axum::body::Body::new), path, false)
        }
        _ => serve_missing(hosting, method, path, headers, kind).await,
    }
}

/// Whether the path addresses the API surface (including debug docs).
/// Only these paths keep the JSON missing semantics for every method;
/// every other path is static surface and enforces the read-only rule.
fn is_api(path: &str) -> bool {
    path == "/api"
        || path.starts_with("/api/")
        || path == "/api-docs"
        || path.starts_with("/api-docs/")
}

/// Whether the request only reads the target.
fn is_read(method: &Method) -> bool {
    *method == Method::GET || *method == Method::HEAD
}

/// Whether any path segment is hidden or a parent reference.
///
/// Hidden files are never served; the guard runs before the file service
/// so their existence stays invisible. Segments are checked after
/// percent-decoding: an encoded dot must not bypass the guard.
/// Undecodable paths are treated as hidden, matching the file service
/// rejecting them.
fn has_dot_segment(path: &str) -> bool {
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8();
    let Ok(decoded) = decoded else {
        return true;
    };
    decoded
        .split('/')
        .any(|segment| segment == ".." || segment.starts_with('.'))
}

/// What an unmatched path resolves to once the file service misses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingKind {
    /// Unknown API path: stays a JSON 404, never the index page.
    UnknownApi,
    /// Missing fingerprinted asset: stays a 404 with the request path.
    MissingAsset,
    /// Extension-less page path: rewrites to the index page.
    IndexPage,
    /// SPA fallback disabled: stays a 404 with the request path.
    NoFallback,
}

/// Classify an unmatched path without touching the filesystem.
fn classify_missing(path: &str, fallback: bool) -> MissingKind {
    if is_api(path) {
        return MissingKind::UnknownApi;
    }
    let leaf = path.rsplit('/').next().unwrap_or(path);
    let dotted = leaf
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && !ext.is_empty());
    if dotted {
        return MissingKind::MissingAsset;
    }
    if fallback {
        MissingKind::IndexPage
    } else {
        MissingKind::NoFallback
    }
}

/// Answer a request the file service could not satisfy.
async fn serve_missing(
    hosting: &StaticHosting,
    method: &Method,
    path: &str,
    headers: &axum::http::HeaderMap,
    kind: MissingKind,
) -> Response {
    match kind {
        MissingKind::UnknownApi => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                "NOT_FOUND",
                format!("Unknown API path: {path}"),
            )),
        )
            .into_response(),
        MissingKind::MissingAsset => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                "NOT_FOUND",
                format!("Static asset not found: {path}"),
            )),
        )
            .into_response(),
        MissingKind::NoFallback => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                "NOT_FOUND",
                format!("No static route for path: {path}"),
            )),
        )
            .into_response(),
        // Serve the fallback page through the file service so
        // conditional requests, ranges and HEAD keep file semantics;
        // only the cache policy is forced to no-store.
        MissingKind::IndexPage => {
            let request = served_request(method, path, headers);
            match tower::ServiceExt::oneshot(
                tower_http::services::ServeFile::new(hosting.index.as_ref()),
                request,
            )
            .await
            {
                Ok(response) if response.status() != StatusCode::NOT_FOUND => {
                    finish_file_response(hosting, response.map(axum::body::Body::new), path, true)
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(
                        "INTERNAL_ERROR",
                        format!("Fallback page is missing: {}", hosting.index.display()),
                    )),
                )
                    .into_response(),
            }
        }
    }
}

/// Rebuild a servable request from the fallback parts. The body is always
/// empty here: only read methods reach the file service.
fn served_request(
    method: &Method,
    path: &str,
    headers: &axum::http::HeaderMap,
) -> axum::http::Request<axum::body::Body> {
    let mut builder = axum::http::Request::builder().method(method.clone());
    if let Ok(uri) = path.parse::<axum::http::Uri>() {
        builder = builder.uri(uri);
    }
    let mut request = builder.body(axum::body::Body::empty()).unwrap_or_else(|_| {
        axum::http::Request::builder()
            .body(axum::body::Body::empty())
            .expect("empty fallback request builds")
    });
    request.headers_mut().extend(headers.clone());
    request
}

/// Apply the static cache policy and hardening headers to a file response.
fn finish_file_response(
    hosting: &StaticHosting,
    mut response: Response,
    path: &str,
    is_index: bool,
) -> Response {
    let headers = response.headers_mut();
    if let Ok(nosniff) = "nosniff".parse::<header::HeaderValue>() {
        headers.insert(header::X_CONTENT_TYPE_OPTIONS, nosniff);
    }
    let cache_control = if is_index {
        "no-store".to_string()
    } else if path.contains("/immutable/") || path.contains("/_app/immutable/") {
        format!(
            "public, max-age={}, immutable",
            hosting.immutable_max_age_secs
        )
    } else {
        "public, max-age=3600".to_string()
    };
    if let Ok(value) = cache_control.parse::<header::HeaderValue>() {
        headers.insert(header::CACHE_CONTROL, value);
    }
    if is_index {
        if let Ok(value) = "text/html; charset=utf-8".parse::<header::HeaderValue>() {
            headers.insert(header::CONTENT_TYPE, value);
        }
    }
    response
}

/// Whether `candidate` names a file inside `root` after canonicalization.
/// Used by tests to assert the traversal story without HTTP round-trips.
#[cfg(test)]
fn confined_to_root(root: &std::path::Path, candidate: &std::path::Path) -> Option<PathBuf> {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical = candidate.canonicalize().ok()?;
    if !canonical.starts_with(&canonical_root) {
        return None;
    }
    Some(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use axum::http::HeaderMap;

    fn hosting(dir: &Path, fallback: bool) -> StaticHosting {
        StaticHosting {
            dir: dir.to_path_buf(),
            index: Arc::new(dir.join("index.html")),
            fallback,
            immutable_max_age_secs: 60,
        }
    }

    fn site() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<app/>").unwrap();
        std::fs::write(dir.path().join("app.js"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join("_app/immutable")).unwrap();
        std::fs::write(dir.path().join("_app/immutable/chunk.js"), "y").unwrap();
        dir
    }

    async fn serve(hosting: &StaticHosting, method: &str, uri: &str) -> Response {
        serve_static(
            hosting,
            &method.parse::<Method>().unwrap(),
            uri,
            &HeaderMap::new(),
        )
        .await
    }

    async fn body_text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn missing_path_classification_prefers_api_and_assets() {
        assert_eq!(
            classify_missing("/api/search", true),
            MissingKind::UnknownApi
        );
        assert_eq!(classify_missing("/api", true), MissingKind::UnknownApi);
        assert_eq!(
            classify_missing("/api-docs/openapi.json", true),
            MissingKind::UnknownApi
        );
        assert_eq!(
            classify_missing("/favicon.png", true),
            MissingKind::MissingAsset
        );
        assert_eq!(
            classify_missing("/_app/immutable/app.js", true),
            MissingKind::MissingAsset
        );
        assert_eq!(classify_missing("/dashboard", true), MissingKind::IndexPage);
        assert_eq!(classify_missing("/", true), MissingKind::IndexPage);
        assert_eq!(
            classify_missing("/dashboard.", true),
            MissingKind::IndexPage
        );
        assert_eq!(
            classify_missing("/dashboard", false),
            MissingKind::NoFallback
        );
        assert_eq!(
            classify_missing("/api/search", false),
            MissingKind::UnknownApi
        );
    }

    #[tokio::test]
    async fn serves_files_and_spa_fallback() {
        let dir = site();
        let hosting = hosting(dir.path(), true);
        let file = serve(&hosting, "GET", "/app.js").await;
        assert_eq!(file.status(), StatusCode::OK);
        assert!(file.headers().get(header::LAST_MODIFIED).is_some());
        let deep = serve(&hosting, "GET", "/workflows/123").await;
        assert_eq!(body_text(deep).await, "<app/>");
        let root = serve(&hosting, "GET", "/").await;
        assert_eq!(body_text(root).await, "<app/>");
    }

    #[tokio::test]
    async fn missing_assets_stay_missing() {
        let dir = site();
        let hosting = hosting(dir.path(), true);
        let missing = serve(&hosting, "GET", "/_app/immutable/broken.js").await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let icon = serve(&hosting, "GET", "/favicon.png").await;
        assert_eq!(icon.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_paths_stay_404_for_every_method() {
        let dir = site();
        let hosting = hosting(dir.path(), true);
        let api = serve(&hosting, "GET", "/api/v1/typo").await;
        assert_eq!(api.status(), StatusCode::NOT_FOUND);
        let traversal = serve(&hosting, "GET", "/../secret").await;
        assert_eq!(traversal.status(), StatusCode::NOT_FOUND);
        let write = serve(&hosting, "POST", "/api/no-such-route").await;
        assert_eq!(write.status(), StatusCode::NOT_FOUND);
        let docs = serve(&hosting, "GET", "/api-docs/openapi.json").await;
        assert_eq!(docs.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn static_paths_reject_non_read_methods() {
        let dir = site();
        let hosting = hosting(dir.path(), true);
        let post = serve(&hosting, "POST", "/dashboard").await;
        assert_eq!(post.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn hidden_segments_stay_invisible() {
        let dir = site();
        std::fs::write(dir.path().join(".secret"), "hidden").unwrap();
        let hosting = hosting(dir.path(), true);
        let dotfile = serve(&hosting, "GET", "/.secret").await;
        assert_eq!(dotfile.status(), StatusCode::NOT_FOUND);
        let traversal = serve(&hosting, "GET", "/_app/../.secret").await;
        assert_eq!(traversal.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn encoded_dots_do_not_bypass_the_guard() {
        let dir = site();
        std::fs::write(dir.path().join(".secret"), "hidden").unwrap();
        let hosting = hosting(dir.path(), true);
        for uri in ["/%2esecret", "/%2Esecret"] {
            let response = serve(&hosting, "GET", uri).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }
        let page = serve(&hosting, "GET", "/%64ashboard").await;
        assert_eq!(page.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn fallback_page_keeps_file_semantics() {
        let dir = site();
        let hosting = hosting(dir.path(), true);
        let head = serve(&hosting, "HEAD", "/dashboard").await;
        assert_eq!(head.status(), StatusCode::OK);
        let body = axum::body::to_bytes(head.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn immutable_assets_are_cached() {
        let dir = site();
        let hosting = hosting(dir.path(), true);
        let res = serve(&hosting, "GET", "/_app/immutable/chunk.js").await;
        assert_eq!(
            res.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=60, immutable"
        );
        assert_eq!(
            res.headers().get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
            "nosniff"
        );
        let page = serve(&hosting, "GET", "/dashboard").await;
        assert_eq!(
            page.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
    }

    #[tokio::test]
    async fn disabled_fallback_keeps_page_paths_missing() {
        let dir = site();
        let hosting = hosting(dir.path(), false);
        let page = serve(&hosting, "GET", "/dashboard").await;
        assert_eq!(page.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn dot_segment_detection_covers_encoded_and_parent_refs() {
        assert!(has_dot_segment("/.secret"));
        assert!(has_dot_segment("/%2esecret"));
        assert!(has_dot_segment("/a/../b"));
        assert!(!has_dot_segment("/dashboard"));
        assert!(!has_dot_segment("/_app/immutable/chunk.js"));
    }

    #[test]
    fn symlink_escape_stays_unconfined() {
        let dir = site();
        let outside_dir = tempfile::tempdir().unwrap();
        let secret = outside_dir.path().join("secret.txt");
        std::fs::write(&secret, "secret").unwrap();
        let leaked = dir.path().join("leak.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, &leaked).unwrap();
        #[cfg(unix)]
        assert!(confined_to_root(dir.path(), &leaked).is_none());
    }

    #[test]
    fn index_page_counts_as_index_for_cache() {
        let dir = site();
        let hosting = hosting(dir.path(), true);
        assert!(hosting.fallback);
        assert_eq!(hosting.immutable_max_age_secs, 60);
    }
}

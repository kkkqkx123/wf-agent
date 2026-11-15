//! Optional static-file hosting with SPA fallback for the web frontend.
//!
//! Disabled unless `ServerConfig::static_dir` is set (`--static-dir`,
//! `WF_SERVER_STATIC_DIR` env or `server.toml`). When enabled, unmatched
//! non-API routes serve files from the directory and fall back to
//! `index.html` so deep routes survive refresh; unknown `/api/*` paths keep
//! the empty 404 so API clients never receive HTML.
//!
//! Static responses carry an `ETag` derived from file size and mtime plus a
//! `Cache-Control` policy: immutable hashed assets are cached forever, other
//! assets get a short TTL, and `index.html` (including the SPA fallback) is
//! never cached. `If-None-Match` is honored with `304 Not Modified`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// Serve one unmatched path from `root`.
pub(crate) async fn serve_static(
    root: &Path,
    path: &str,
    if_none_match: Option<&str>,
) -> Response {
    if path.starts_with("/api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut candidate = PathBuf::from(root);
    for part in path.trim_start_matches('/').split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return StatusCode::NOT_FOUND.into_response();
        }
        candidate.push(part);
    }
    if candidate.is_dir() {
        candidate.push("index.html");
    }
    let Some(served) = confine_to_root(root, &candidate) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (bytes, served, is_index) = match read_file(served.clone()).await {
        Some(bytes) => (bytes, served.clone(), served == root.join("index.html") || path == "/"),
        None => {
            // SPA fallback: serve the app shell for unknown app routes.
            let index = root.join("index.html");
            let Some(index) = confine_to_root(root, &index) else {
                return StatusCode::NOT_FOUND.into_response();
            };
            match read_file(index.clone()).await {
                Some(bytes) => (bytes, index, true),
                None => return StatusCode::NOT_FOUND.into_response(),
            }
        }
    };

    let etag = file_etag(&served);
    if let Some(supplied) = if_none_match {
        if etag_matches(supplied, &etag) {
            let mut response = StatusCode::NOT_MODIFIED.into_response();
            insert_cache_headers(&mut response, &served, path, &etag, is_index, true);
            return response;
        }
    }

    let mut response = bytes.into_response();
    insert_cache_headers(&mut response, &served, path, &etag, is_index, false);
    if let Some(content_type) = content_type(served.as_path()) {
        if let Ok(value) = content_type.parse::<header::HeaderValue>() {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
    }
    response
}

async fn read_file(path: PathBuf) -> Option<Vec<u8>> {
    tokio::task::spawn_blocking(move || std::fs::read(path).ok())
        .await
        .ok()
        .flatten()
}

fn file_etag(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(meta) => {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("W/\"{}-{}\"", meta.len(), mtime)
        }
        Err(_) => "W/\"0-0\"".to_string(),
    }
}

fn etag_matches(supplied: &str, etag: &str) -> bool {
    supplied
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == etag || candidate == "*" )
}

fn insert_cache_headers(
    response: &mut Response,
    served: &Path,
    path: &str,
    etag: &str,
    is_index: bool,
    not_modified: bool,
) {
    if let Ok(value) = etag.parse::<header::HeaderValue>() {
        response.headers_mut().insert(header::ETAG, value);
    }
    let cache_control = if is_index || served.ends_with("index.html") {
        "no-cache"
    } else if path.contains("/immutable/") || path.contains("/_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };
    if let Ok(value) = cache_control.parse::<header::HeaderValue>() {
        response.headers_mut().insert(header::CACHE_CONTROL, value);
    }
    if !not_modified {
        // keep content type insertion in caller; nothing else here
    }
}

/// Resolve `candidate` against the canonical site root: the nearest existing
/// ancestor is canonicalized and must stay inside the root, then the
/// remaining suffix is re-appended and re-checked so links inside the site
/// cannot carry reads outside it. Returns the confined path on success.
fn confine_to_root(root: &Path, candidate: &Path) -> Option<PathBuf> {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut anchor = candidate;
    let mut suffix = PathBuf::new();
    loop {
        if anchor.exists() {
            break;
        }
        match anchor.parent() {
            Some(parent) if parent != anchor => {
                if let Some(name) = anchor.file_name() {
                    let mut next = PathBuf::from(name);
                    next.push(suffix);
                    suffix = next;
                }
                anchor = parent;
            }
            _ => return None,
        }
    }
    let canonical_anchor = anchor.canonicalize().ok()?;
    if !canonical_anchor.starts_with(&canonical_root) {
        return None;
    }
    // An empty suffix must not be joined: joining it would append a
    // trailing separator and turn file reads into directory lookups.
    let resolved = if suffix.as_os_str().is_empty() {
        canonical_anchor
    } else {
        canonical_anchor.join(suffix)
    };
    if !resolved.starts_with(&canonical_root) {
        return None;
    }
    Some(resolved)
}

fn content_type(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => Some("text/html; charset=utf-8"),
        Some("js" | "mjs") => Some("text/javascript; charset=utf-8"),
        Some("css") => Some("text/css; charset=utf-8"),
        Some("json" | "map") => Some("application/json"),
        Some("jsonld") => Some("application/ld+json"),
        Some("svg") => Some("image/svg+xml"),
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("avif") => Some("image/avif"),
        Some("ico") => Some("image/x-icon"),
        Some("woff") => Some("font/woff"),
        Some("woff2") => Some("font/woff2"),
        Some("ttf" | "otf") => Some("font/ttf"),
        Some("wasm") => Some("application/wasm"),
        Some("mp4") => Some("video/mp4"),
        Some("webm") => Some("video/webm"),
        Some("ogg" | "oga" | "ogv") => Some("application/ogg"),
        Some("txt" | "text") => Some("text/plain; charset=utf-8"),
        Some("webmanifest") => Some("application/manifest+json"),
        Some("xml") => Some("application/xml"),
        Some("pdf") => Some("application/pdf"),
        Some("zip") => Some("application/zip"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<app/>").unwrap();
        std::fs::write(dir.path().join("app.js"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join("_app/immutable")).unwrap();
        std::fs::write(dir.path().join("_app/immutable/chunk.js"), "y").unwrap();
        dir
    }

    async fn body_text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn serves_files_and_spa_fallback() {
        let dir = site();
        let file = serve_static(dir.path(), "/app.js", None).await;
        assert_eq!(
            file.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );
        assert!(file.headers().get(header::ETAG).is_some());
        let deep = serve_static(dir.path(), "/workflows/123", None).await;
        assert_eq!(body_text(deep).await, "<app/>");
        let root = serve_static(dir.path(), "/", None).await;
        assert_eq!(body_text(root).await, "<app/>");
    }

    #[tokio::test]
    async fn api_paths_and_traversal_stay_404() {
        let dir = site();
        let api = serve_static(dir.path(), "/api/v1/typo", None).await;
        assert_eq!(api.status(), StatusCode::NOT_FOUND);
        let traversal = serve_static(dir.path(), "/../secret", None).await;
        assert_eq!(traversal.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn honors_if_none_match() {
        let dir = site();
        let first = serve_static(dir.path(), "/app.js", None).await;
        let etag = first.headers().get(header::ETAG).unwrap().to_str().unwrap().to_string();
        let second = serve_static(dir.path(), "/app.js", Some(&etag)).await;
        assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn immutable_assets_are_cached() {
        let dir = site();
        let res = serve_static(dir.path(), "/_app/immutable/chunk.js", None).await;
        assert_eq!(
            res.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_escape_stays_404() {
        let dir = site();
        let outside_dir = tempfile::tempdir().unwrap();
        let secret = outside_dir.path().join("secret.txt");
        std::fs::write(&secret, "secret").unwrap();
        std::os::unix::fs::symlink(&secret, dir.path().join("leak.txt")).unwrap();
        let leaked = serve_static(dir.path(), "/leak.txt", None).await;
        assert_eq!(leaked.status(), StatusCode::NOT_FOUND);
        std::os::unix::fs::symlink(outside_dir.path(), dir.path().join("evildir")).unwrap();
        let dir_leak = serve_static(dir.path(), "/evildir/secret.txt", None).await;
        assert_eq!(dir_leak.status(), StatusCode::NOT_FOUND);
    }
}

//! Optional static-file hosting with SPA fallback for the web frontend.
//!
//! Disabled unless `ServerConfig::static_dir` is set (`--static-dir`,
//! `WF_SERVER_STATIC_DIR` env or `server.toml`). When enabled, unmatched
//! non-API routes serve files from the directory and fall back to
//! `index.html` so deep routes survive refresh; unknown `/api/*` paths keep
//! the empty 404 so API clients never receive HTML.

use std::path::{Path, PathBuf};

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// Serve one unmatched path from `root`.
pub(crate) async fn serve_static(root: &Path, path: &str) -> Response {
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
    let bytes = read_file(candidate.clone()).await;
    let (bytes, served) = match bytes {
        Some(bytes) => (bytes, candidate),
        None => {
            // SPA fallback: serve the app shell for unknown app routes.
            let index = root.join("index.html");
            match read_file(index.clone()).await {
                Some(bytes) => (bytes, index),
                None => return StatusCode::NOT_FOUND.into_response(),
            }
        }
    };
    let mut response = bytes.into_response();
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

fn content_type(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => Some("text/html; charset=utf-8"),
        Some("js" | "mjs") => Some("text/javascript; charset=utf-8"),
        Some("css") => Some("text/css; charset=utf-8"),
        Some("json" | "map") => Some("application/json"),
        Some("svg") => Some("image/svg+xml"),
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("ico") => Some("image/x-icon"),
        Some("woff") => Some("font/woff"),
        Some("woff2") => Some("font/woff2"),
        Some("ttf") => Some("font/ttf"),
        Some("txt") => Some("text/plain; charset=utf-8"),
        Some("webmanifest") => Some("application/manifest+json"),
        Some("xml") => Some("application/xml"),
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
        let file = serve_static(dir.path(), "/app.js").await;
        assert_eq!(
            file.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );
        let deep = serve_static(dir.path(), "/workflows/123").await;
        assert_eq!(body_text(deep).await, "<app/>");
        let root = serve_static(dir.path(), "/").await;
        assert_eq!(body_text(root).await, "<app/>");
    }

    #[tokio::test]
    async fn api_paths_and_traversal_stay_404() {
        let dir = site();
        let api = serve_static(dir.path(), "/api/v1/typo").await;
        assert_eq!(api.status(), StatusCode::NOT_FOUND);
        let traversal = serve_static(dir.path(), "/../secret").await;
        assert_eq!(traversal.status(), StatusCode::NOT_FOUND);
    }
}

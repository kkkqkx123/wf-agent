//! Single-host integration test: one port serves the app shell and the
//! API side by side, with API precedence on conflicts and JSON missing
//! semantics for unknown API paths.

use std::sync::Arc;

use wf_metrics::MetricsRegistry;

fn test_ctx() -> Arc<wf_api::ApiContext> {
    let ctx = wf_api::ApiContext::new(
        wf_storage::context::StorageContext::new_memory(),
        Arc::new(wf_resource::registry::ResourceRegistries::new()),
    );
    Arc::new(
        ctx.with_user_git(wf_types::config::user_git::UserGitConfig {
            enabled: true,
            ..Default::default()
        }),
    )
}

fn test_dirs() -> Arc<wf_server::directory::DirectoryContext> {
    Arc::new(wf_server::directory::DirectoryContext::new(
        wf_server::directory::DirectoryConfig::default(),
    ))
}

fn write_site(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("_app/immutable")).expect("fixture dirs");
    std::fs::write(dir.join("index.html"), "<html>app</html>").expect("index fixture");
    std::fs::write(dir.join("_app/immutable/app.js"), "console.log(1)").expect("asset fixture");
}

fn static_config(dir: &std::path::Path) -> wf_server::StaticConfig {
    wf_server::StaticConfig {
        dir: dir.to_path_buf(),
        index: "index.html".to_string(),
        fallback: true,
        immutable_max_age_secs: 60,
    }
}

async fn serve_single(dir: &std::path::Path) -> wf_server::ServerHandle {
    let mut config = wf_server::ServerConfig::default();
    config.bind_addr = "127.0.0.1:0".parse().unwrap();
    config.static_config = Some(static_config(dir));
    wf_server::serve_full_with_config(
        Arc::new(MetricsRegistry::new()),
        test_ctx(),
        &config,
        test_dirs(),
    )
    .await
    .expect("single-host server should bind")
}

#[tokio::test]
async fn single_host_serves_pages_beside_api() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_site(dir.path());
    std::fs::write(dir.path().join(".secret"), "hidden").expect("dotfile fixture");
    let handle = serve_single(dir.path()).await;
    let base = format!("http://{}", handle.addr());
    let client = reqwest::Client::new();

    let root = client
        .get(format!("{base}/"))
        .send()
        .await
        .expect("root should respond");
    assert_eq!(root.status(), reqwest::StatusCode::OK);
    assert!(
        root.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .is_some_and(|v| v.to_str().unwrap_or_default().contains("text/html")),
        "root serves the app shell, not the JSON root answer"
    );

    let page = client
        .get(format!("{base}/dashboard"))
        .send()
        .await
        .expect("spa path should respond");
    assert_eq!(page.status(), reqwest::StatusCode::OK);

    let asset = client
        .get(format!("{base}/_app/immutable/app.js"))
        .send()
        .await
        .expect("asset should respond");
    assert_eq!(asset.status(), reqwest::StatusCode::OK);
    assert!(
        asset
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .is_some_and(|v| v.to_str().unwrap_or_default().contains("immutable")),
        "fingerprinted assets stay immutable"
    );

    let info = client
        .get(format!("{base}/api/v1/info"))
        .send()
        .await
        .expect("info should respond");
    assert_eq!(info.status(), reqwest::StatusCode::OK);

    let health = client
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("health should respond");
    assert_eq!(health.status(), reqwest::StatusCode::OK);

    let unknown = client
        .get(format!("{base}/api/no-such-route"))
        .send()
        .await
        .expect("unknown api should respond");
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);

    let write = client
        .post(format!("{base}/api/no-such-route"))
        .send()
        .await
        .expect("unknown api write should respond");
    assert_eq!(write.status(), reqwest::StatusCode::NOT_FOUND);

    let missing_asset = client
        .get(format!("{base}/_app/immutable/broken.js"))
        .send()
        .await
        .expect("missing asset should respond");
    assert_eq!(missing_asset.status(), reqwest::StatusCode::NOT_FOUND);

    let write_page = client
        .post(format!("{base}/dashboard"))
        .send()
        .await
        .expect("static write should respond");
    assert_eq!(write_page.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);

    let dotfile = client
        .get(format!("{base}/.secret"))
        .send()
        .await
        .expect("dotfile should respond");
    assert_eq!(dotfile.status(), reqwest::StatusCode::NOT_FOUND);

    handle.shutdown().await;
}

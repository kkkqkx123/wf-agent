//! Integration tests exercising the real bind/accept/HTTP path of
//! `wf-server::serve`, plus the bind-failure path.

use std::net::TcpListener;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wf_metrics::MetricsRegistry;
use wf_server::{serve, ServeError};

/// Minimal registry with one completed workflow execution so the Prometheus
/// body contains `workflow.execution.count`.
fn seeded_registry() -> Arc<MetricsRegistry> {
    let registry = Arc::new(MetricsRegistry::new());
    registry.workflow().record_execution_start("e1", "wf-1");
    registry
        .workflow()
        .record_execution_complete("e1", "wf-1", None, true, 100.0, None);
    registry
}

#[tokio::test]
async fn serve_binds_and_responds() {
    let registry = seeded_registry();
    let handle = serve(registry, "127.0.0.1:0".parse().unwrap())
        .await
        .expect("serve should bind to a random port");
    let addr = handle.addr();

    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("client should connect");
    stream
        .write_all(b"GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .expect("request should be written");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("response should be readable");
    let text = String::from_utf8(response).expect("response should be utf-8");

    assert!(
        text.starts_with("HTTP/1.1 200"),
        "status line should be 200: {text}"
    );
    assert!(
        text.to_lowercase().contains("content-type: text/plain"),
        "content type should be prometheus text: {text}"
    );
    assert!(
        text.contains("workflow.execution.count"),
        "body should contain the workflow counter: {text}"
    );

    handle.shutdown().await;
}

#[tokio::test]
async fn serve_reports_bind_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("probe listener should bind");
    let occupied = listener.local_addr().expect("probe listener should report addr");

    let registry = Arc::new(MetricsRegistry::new());
    let result = serve(registry, occupied).await;
    assert!(
        matches!(result, Err(ServeError::Bind(_))),
        "serving on an occupied port must fail with ServeError::Bind"
    );
}

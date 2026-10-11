//! Unit tests for [`RestExecutor`]: URL building, dispatch, retries and
//! interceptor chains.

use super::rest::*;
use super::test_util::mock_server;
use crate::error::ToolError;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use wf_types::tool::ToolExecutionOptions;

fn make_rest_tool(id: &str, config: Value) -> wf_types::tool::Tool {
    wf_types::tool::Tool {
        id: id.into(),
        name: id.into(),
        description: "REST tool".into(),
        tool_type: wf_types::tool::ToolType::Rest,
        parameters: None,
        metadata: None,
        config: Some(config),
        enabled: Some(true),
        strict: None,
        default_timeout_ms: None,
    }
}

fn make_options() -> ToolExecutionOptions {
    ToolExecutionOptions {
        timeout: Some(5000),
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    }
}

#[tokio::test]
async fn test_get_method_dispatch_and_url() {
    let (addr, rx) = mock_server(r#"{"ok":true}"#, "200 OK");
    let tool = make_rest_tool(
        "rest_get",
        serde_json::json!({ "base_url": format!("http://{}", addr) }),
    );
    let executor = RestExecutor::new().expect("build REST executor for test");
    let ctx = ToolExecutionContext::new("e1".into());

    let result = executor
        .execute(
            &tool,
            &serde_json::json!({
                "url": "/api/data",
                "method": "GET",
                "query": { "a": 1, "b": "x" }
            }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();

    assert!(result.success, "expected success, got {:?}", result.error);
    let head = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    assert!(
        head.starts_with("GET /api/data?a=1&b=x HTTP/1.1"),
        "head: {}",
        head
    );
}

#[tokio::test]
async fn test_post_body_and_headers() {
    let (addr, rx) = mock_server(r#"{"ok":true}"#, "200 OK");
    let tool = make_rest_tool(
        "rest_post",
        serde_json::json!({ "base_url": format!("http://{}", addr) }),
    );
    let executor = RestExecutor::new().expect("build REST executor for test");
    let ctx = ToolExecutionContext::new("e1".into());

    let result = executor
        .execute(
            &tool,
            &serde_json::json!({
                "url": "/api/write",
                "method": "POST",
                "body": { "key": "value" },
                "headers": { "X-Test": "yes" }
            }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();

    assert!(result.success, "expected success, got {:?}", result.error);
    let head = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    assert!(
        head.starts_with("POST /api/write HTTP/1.1"),
        "head: {}",
        head
    );
    assert!(head.contains("x-test: yes"), "head: {}", head);
}

#[tokio::test]
async fn test_build_full_url_without_query() {
    let url = build_full_url("http://example.com", "v1/users", None);
    assert_eq!(url, "http://example.com/v1/users");

    let url = build_full_url("http://example.com/", "/v1/users", None);
    assert_eq!(url, "http://example.com/v1/users");

    // Absolute URL overrides base.
    let url = build_full_url("http://example.com", "https://other.dev/x", None);
    assert_eq!(url, "https://other.dev/x");
}

#[tokio::test]
async fn test_build_full_url_with_query() {
    let url = build_full_url(
        "http://example.com",
        "v1/search",
        Some(&serde_json::json!({ "q": "a b", "limit": 10, "null_skip": null })),
    );
    assert_eq!(url, "http://example.com/v1/search?limit=10&q=a%20b");

    // Appending to an existing query string.
    let url = build_full_url(
        "http://example.com",
        "v1/search?already=1",
        Some(&serde_json::json!({ "x": "2" })),
    );
    assert_eq!(url, "http://example.com/v1/search?already=1&x=2");
}

#[tokio::test]
async fn test_error_interceptor_chain() {
    let (addr, _rx) = mock_server("nope", "404 Not Found");
    let tool = make_rest_tool(
        "rest_err",
        serde_json::json!({ "base_url": format!("http://{}", addr) }),
    );
    let mut executor = RestExecutor::new().expect("build REST executor for test");
    executor.add_error_interceptor(Arc::new(|e| {
        ToolError::Internal(format!("intercepted: {}", e))
    }));
    let ctx = ToolExecutionContext::new("e1".into());

    // A 404 produces a non-ok ToolExecutionResult, not an Err; use an
    // unsupported method to exercise the interceptor on the Err path.
    let result = executor
        .execute(
            &tool,
            &serde_json::json!({ "url": "/x", "method": "TRACE" }),
            &make_options(),
            &ctx,
        )
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("intercepted:"), "err: {}", err);
}

#[tokio::test]
async fn test_retry_then_success() {
    // First request fails with 500, then succeeds.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut attempt = 0;
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            attempt += 1;
            if attempt == 1 {
                let resp =
                    "HTTP/1.1 500 Server Error \r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes());
            } else {
                let body = r#"{"ok":true}"#;
                let resp = format!(
                        "HTTP/1.1 200 OK \r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                let _ = stream.write_all(resp.as_bytes());
            }
        }
    });

    let tool = make_rest_tool(
        "rest_retry",
        serde_json::json!({
            "base_url": format!("http://{}", addr),
            "max_retries": 2,
            "retry_delay": 10,
        }),
    );
    let executor = RestExecutor::new().expect("build REST executor for test");
    let ctx = ToolExecutionContext::new("e1".into());

    let result = executor
        .execute(
            &tool,
            &serde_json::json!({ "url": "/api/retry", "method": "GET" }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();

    assert!(
        result.success,
        "expected eventual success: {:?}",
        result.error
    );
    assert_eq!(result.retry_count, 1);
}

#[tokio::test]
async fn test_retry_exhausted() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let resp =
                "HTTP/1.1 503 Unavailable \r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let tool = make_rest_tool(
        "rest_exhaust",
        serde_json::json!({
            "base_url": format!("http://{}", addr),
            "max_retries": 2,
            "retry_delay": 10,
        }),
    );
    let executor = RestExecutor::new().expect("build REST executor for test");
    let ctx = ToolExecutionContext::new("e1".into());

    let result = executor
        .execute(
            &tool,
            &serde_json::json!({ "url": "/api/fail", "method": "GET" }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();

    assert!(!result.success);
    assert_eq!(result.retry_count, 2);
    let msg = result.error.unwrap();
    assert!(msg.contains("server_error"), "msg: {}", msg);
    assert!(msg.contains("503"), "msg: {}", msg);
    assert_eq!(result.retryable, Some(true));
}

#[tokio::test]
async fn test_client_error_does_not_retry() {
    let (addr, _rx) = mock_server("missing", "404 Not Found");
    let tool = make_rest_tool(
        "rest_404",
        serde_json::json!({
            "base_url": format!("http://{}", addr),
            "max_retries": 2,
            "retry_delay": 10,
        }),
    );
    let executor = RestExecutor::new().expect("build REST executor for test");
    let ctx = ToolExecutionContext::new("e1".into());

    let result = executor
        .execute(
            &tool,
            &serde_json::json!({ "url": "/api/missing", "method": "GET" }),
            &make_options(),
            &ctx,
        )
        .await
        .unwrap();

    assert!(!result.success);
    assert_eq!(result.retry_count, 0, "4xx must not consume retries");
    assert_eq!(result.retryable, Some(false));
    let msg = result.error.unwrap();
    assert!(msg.contains("not_found"), "msg: {}", msg);
    assert!(
        !msg.contains("[retryable]"),
        "message must not carry protocol: {msg}"
    );
}

#[tokio::test]
async fn test_classify_status() {
    assert_eq!(
        classify_status(reqwest::StatusCode::from_u16(429).unwrap()),
        RestErrorKind::RateLimited
    );
    assert_eq!(
        classify_status(reqwest::StatusCode::from_u16(500).unwrap()),
        RestErrorKind::ServerError
    );
    assert_eq!(
        classify_status(reqwest::StatusCode::from_u16(404).unwrap()),
        RestErrorKind::NotFound
    );
    assert_eq!(
        classify_status(reqwest::StatusCode::from_u16(403).unwrap()),
        RestErrorKind::Unauthorized
    );
    assert_eq!(
        classify_status(reqwest::StatusCode::from_u16(422).unwrap()),
        RestErrorKind::ClientError
    );
}

//! Shared integration test helpers: a dependency-free HTTP/1.1 mock server
//! built on tokio's TcpListener. It records every request and dispatches to a
//! caller-provided handler, so tests can exercise the real `LlmClientImpl`
//! HTTP path (retries, timeouts, SSE streaming) without network access.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A parsed inbound HTTP request (body as UTF-8 lossy text).
#[derive(Debug, Clone)]
pub struct MockRequest {
    pub method: String,
    pub path: String,
    pub body: String,
}

/// The response to send back to the client.
pub enum MockResponse {
    /// Plain HTTP response with a body (JSON in practice).
    Json { status: u16, body: String },
    /// Server-sent-events stream; each event is sent as one `data:` frame.
    Sse { status: u16, events: Vec<String> },
    /// JSON response sent after an asynchronous delay (timeout tests).
    Delayed {
        status: u16,
        body: String,
        delay: Duration,
    },
}

impl MockResponse {
    pub fn ok_json(body: impl Into<String>) -> Self {
        Self::Json {
            status: 200,
            body: body.into(),
        }
    }

    pub fn status(status: u16, body: impl Into<String>) -> Self {
        Self::Json {
            status,
            body: body.into(),
        }
    }

    /// A JSON response that is only sent after `delay` (for timeout tests).
    /// The delay is asynchronous, so it never blocks the runtime.
    pub fn delayed_json(status: u16, body: impl Into<String>, delay: Duration) -> Self {
        Self::Delayed {
            status,
            body: body.into(),
            delay,
        }
    }
}

type Handler = Arc<dyn Fn(&MockRequest) -> MockResponse + Send + Sync>;

/// A tiny HTTP mock server bound to an ephemeral local port.
pub struct MockServer {
    pub addr: SocketAddr,
    requests: Arc<Mutex<Vec<MockRequest>>>,
    call_count: Arc<AtomicUsize>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MockServer {
    /// Spawn a server whose handler is called for every request. Must be
    /// awaited from inside a tokio runtime.
    pub async fn spawn<F>(handler: F) -> Self
    where
        F: Fn(&MockRequest) -> MockResponse + Send + Sync + 'static,
    {
        let handler: Handler = Arc::new(handler);
        let requests: Arc<Mutex<Vec<MockRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let call_count = Arc::new(AtomicUsize::new(0));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");
        let requests_loop = requests.clone();
        let call_count_loop = call_count.clone();

        tokio::spawn(async move {
            let mut shutdown = shutdown_rx;
            loop {
                tokio::select! {
                    _ = &mut shutdown => break,
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.expect("accept");
                        let requests = requests_loop.clone();
                        let handler = handler.clone();
                        let call_count = call_count_loop.clone();
                        tokio::spawn(async move {
                            handle_connection(stream, requests, handler, call_count).await;
                        });
                    }
                }
            }
        });

        Self {
            addr,
            requests,
            call_count,
            shutdown: Some(shutdown_tx),
        }
    }

    /// Full URL for a path on this server.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    /// All requests received so far, in order.
    pub fn requests(&self) -> Vec<MockRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Number of requests received so far.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(sender) = self.shutdown.take() {
            let _ = sender.send(());
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<MockRequest>>>,
    handler: Handler,
    call_count: Arc<AtomicUsize>,
) {
    let request = match read_request(&mut stream).await {
        Some(request) => request,
        None => return,
    };

    requests.lock().unwrap().push(request.clone());
    call_count.fetch_add(1, Ordering::SeqCst);

    let response = handler(&request);
    let _ = write_response(&mut stream, response).await;
}

async fn read_request(stream: &mut TcpStream) -> Option<MockRequest> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
    }

    let header_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("header terminator");

    let head = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut content_length = 0usize;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    while buf.len() < header_end + 4 + content_length {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }

    let body = String::from_utf8_lossy(&buf[header_end + 4..]).to_string();
    Some(MockRequest { method, path, body })
}

async fn write_response(stream: &mut TcpStream, response: MockResponse) {
    match response {
        MockResponse::Json { status, body } => {
            let reason = reason_phrase(status);
            let head = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        }
        MockResponse::Delayed {
            status,
            body,
            delay,
        } => {
            tokio::time::sleep(delay).await;
            let reason = reason_phrase(status);
            let head = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        }
        MockResponse::Sse { status, events } => {
            let head = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.flush().await;
            for event in events {
                let frame = format!("data: {event}\r\n\r\n");
                let _ = stream.write_all(frame.as_bytes()).await;
                let _ = stream.flush().await;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        408 => "Request Timeout",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "OK",
    }
}

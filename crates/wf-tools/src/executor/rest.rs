use async_trait::async_trait;
use reqwest::RequestBuilder;
use serde_json::Value;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::runtime_config::RestToolConfig;
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;
use wf_types::Metadata;

pub type RequestInterceptor = Arc<dyn Fn(RequestBuilder) -> RequestBuilder + Send + Sync>;
pub type ResponseInterceptor = Arc<dyn Fn(reqwest::Response) -> reqwest::Response + Send + Sync>;
pub type ErrorInterceptor = Arc<dyn Fn(ToolError) -> ToolError + Send + Sync>;

const CIRCUIT_CLOSED: u8 = 0;
const CIRCUIT_OPEN: u8 = 1;
const CIRCUIT_HALF_OPEN: u8 = 2;

/// Typed classification of a non-2xx REST response, mirroring the TS error
/// taxonomy used for circuit breaking and approval decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestErrorKind {
    /// 400 Bad Request — invalid request, not retryable.
    BadRequest,
    /// 401/403 — authentication / authorization failure.
    Unauthorized,
    /// 404 — resource not found.
    NotFound,
    /// 429 — rate limited, retryable with backoff.
    RateLimited,
    /// 5xx — server side failure, retryable.
    ServerError,
    /// Other 4xx — client side failure, not retryable.
    ClientError,
    /// Network / transport failure (connection refused, DNS, timeout).
    NetworkError,
    /// The circuit breaker rejected the call.
    CircuitOpen,
    /// Unclassified error.
    Unknown,
}

impl RestErrorKind {
    /// Whether a retry is likely to succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            RestErrorKind::RateLimited | RestErrorKind::ServerError | RestErrorKind::NetworkError
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RestErrorKind::BadRequest => "bad_request",
            RestErrorKind::Unauthorized => "unauthorized",
            RestErrorKind::NotFound => "not_found",
            RestErrorKind::RateLimited => "rate_limited",
            RestErrorKind::ServerError => "server_error",
            RestErrorKind::ClientError => "client_error",
            RestErrorKind::NetworkError => "network_error",
            RestErrorKind::CircuitOpen => "circuit_open",
            RestErrorKind::Unknown => "unknown",
        }
    }
}

/// Classify a response status code into a typed error kind.
pub fn classify_status(status: reqwest::StatusCode) -> RestErrorKind {
    let code = status.as_u16();
    match code {
        401 | 403 => RestErrorKind::Unauthorized,
        404 => RestErrorKind::NotFound,
        429 => RestErrorKind::RateLimited,
        500..=599 => RestErrorKind::ServerError,
        400 | 402 | 405..=499 => RestErrorKind::ClientError,
        _ => RestErrorKind::Unknown,
    }
}

pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    state: AtomicU8,
    failures: AtomicU32,
    half_open_successes: AtomicU32,
    half_open_required: u32,
    last_failure: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            state: AtomicU8::new(CIRCUIT_CLOSED),
            failures: AtomicU32::new(0),
            half_open_successes: AtomicU32::new(0),
            half_open_required: 1,
            last_failure: Mutex::new(None),
        }
    }

    pub fn is_allowed(&self) -> bool {
        match self.state.load(Ordering::SeqCst) {
            CIRCUIT_CLOSED => true,
            CIRCUIT_OPEN => {
                let last = *self.last_failure.lock().unwrap();
                if let Some(t) = last {
                    if t.elapsed() >= self.reset_timeout {
                        self.state.store(CIRCUIT_HALF_OPEN, Ordering::SeqCst);
                        self.half_open_successes.store(0, Ordering::SeqCst);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CIRCUIT_HALF_OPEN => true,
            _ => false,
        }
    }

    pub fn record_success(&self) {
        match self.state.load(Ordering::SeqCst) {
            CIRCUIT_HALF_OPEN => {
                let successes = self.half_open_successes.fetch_add(1, Ordering::SeqCst) + 1;
                if successes >= self.half_open_required {
                    self.state.store(CIRCUIT_CLOSED, Ordering::SeqCst);
                    self.failures.store(0, Ordering::SeqCst);
                }
            }
            CIRCUIT_CLOSED => {
                self.failures.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    pub fn record_failure(&self) {
        *self.last_failure.lock().unwrap() = Some(Instant::now());
        match self.state.load(Ordering::SeqCst) {
            CIRCUIT_HALF_OPEN => {
                self.state.store(CIRCUIT_OPEN, Ordering::SeqCst);
            }
            CIRCUIT_CLOSED => {
                let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.failure_threshold {
                    self.state.store(CIRCUIT_OPEN, Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }

    pub fn state(&self) -> &str {
        match self.state.load(Ordering::SeqCst) {
            CIRCUIT_CLOSED => "closed",
            CIRCUIT_OPEN => "open",
            CIRCUIT_HALF_OPEN => "half_open",
            _ => "unknown",
        }
    }
}

pub struct RestExecutor {
    client: reqwest::Client,
    circuit_breaker: Option<CircuitBreaker>,
    request_interceptors: Vec<RequestInterceptor>,
    response_interceptors: Vec<ResponseInterceptor>,
    error_interceptors: Vec<ErrorInterceptor>,
}

/// Fully resolved request describing how the REST call should be dispatched.
#[derive(Debug, Clone)]
pub struct RestRequestSpec {
    pub url: String,
    pub method: String,
    pub headers: Option<Metadata>,
    pub body: Option<Value>,
    pub query: Option<Value>,
    pub timeout_ms: u64,
}

impl RestExecutor {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build reqwest client");
        Self {
            client,
            circuit_breaker: None,
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
            error_interceptors: Vec::new(),
        }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            client,
            circuit_breaker: None,
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
            error_interceptors: Vec::new(),
        }
    }

    pub fn with_circuit_breaker(mut self, failure_threshold: u32, reset_timeout: Duration) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::new(failure_threshold, reset_timeout));
        self
    }

    pub fn add_request_interceptor(&mut self, interceptor: RequestInterceptor) {
        self.request_interceptors.push(interceptor);
    }

    pub fn add_response_interceptor(&mut self, interceptor: ResponseInterceptor) {
        self.response_interceptors.push(interceptor);
    }

    pub fn add_error_interceptor(&mut self, interceptor: ErrorInterceptor) {
        self.error_interceptors.push(interceptor);
    }

    pub fn circuit_breaker_state(&self) -> Option<&str> {
        self.circuit_breaker.as_ref().map(|cb| cb.state())
    }

    fn apply_request_interceptors(&self, request: RequestBuilder) -> RequestBuilder {
        let mut req = request;
        for interceptor in &self.request_interceptors {
            req = interceptor(req);
        }
        req
    }

    fn apply_response_interceptors(&self, response: reqwest::Response) -> reqwest::Response {
        let mut resp = response;
        for interceptor in &self.response_interceptors {
            resp = interceptor(resp);
        }
        resp
    }

    fn apply_error_interceptors(&self, error: ToolError) -> ToolError {
        let mut err = error;
        for interceptor in &self.error_interceptors {
            err = interceptor(err);
        }
        err
    }

    fn parse_config(tool: &wf_types::tool::Tool) -> RestToolConfig {
        if let Some(config) = &tool.config {
            if let Ok(rest_config) = serde_json::from_value::<RestToolConfig>(config.clone()) {
                return rest_config;
            }
        }
        RestToolConfig {
            base_url: None,
            headers: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            method: None,
        }
    }

    /// Resolve the request spec from the tool config and the call parameters,
    /// aligned with the TS `RestExecutor.doExecute`:
    ///
    /// - url: `parameters.url` / `parameters.endpoint`, falling back to
    ///   `{base_url}/{tool.name}`;
    /// - method: `parameters.method` (uppercased), falling back to the tool
    ///   config `method`, then `POST` (TS defaults to `GET`, but the previous
    ///   Rust executor used POST; keep config/params override for clarity);
    /// - body / headers / query: taken from the matching parameters.
    fn resolve_spec(
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        config: &RestToolConfig,
    ) -> ToolResult<RestRequestSpec> {
        let base_url = config.base_url.clone().unwrap_or_default();

        let url_param = parameters
            .get("url")
            .and_then(|v| v.as_str())
            .or_else(|| parameters.get("endpoint").and_then(|v| v.as_str()))
            .map(String::from);

        let raw_url = match url_param {
            Some(url) => url,
            None if !tool.name.is_empty() => {
                format!("{}/{}", base_url.trim_end_matches('/'), tool.name)
            }
            None => {
                return Err(ToolError::ValidationFailed(
                    "REST tool requires a 'url' or 'endpoint' parameter, or a tool name".into(),
                ))
            }
        };

        let method = parameters
            .get("method")
            .and_then(|v| v.as_str())
            .map(str::to_uppercase)
            .or_else(|| config.method.clone().map(|m| m.to_uppercase()))
            .unwrap_or_else(|| "POST".into());

        let body = parameters.get("body").cloned();
        let headers = parameters
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .or_else(|| config.headers.clone());
        let query = parameters
            .get("query")
            .or_else(|| parameters.get("params"))
            .cloned();
        let timeout_ms = options.timeout.or(config.timeout).unwrap_or(30000);

        let url = build_full_url(&base_url, &raw_url, query.as_ref());

        Ok(RestRequestSpec {
            url,
            method,
            headers,
            body,
            query,
            timeout_ms,
        })
    }
}

/// Merge a base URL with a (possibly relative) endpoint and append query
/// string parameters, mirroring the TS `buildFullUrl` behaviour.
pub fn build_full_url(base_url: &str, url: &str, query: Option<&Value>) -> String {
    let mut full_url = url.to_string();

    if !base_url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
        let clean_base = base_url.trim_end_matches('/');
        let clean_url = url.trim_start_matches('/');
        full_url = format!("{}/{}", clean_base, clean_url);
    }

    if let Some(query) = query {
        let Value::Object(map) = query else {
            return full_url;
        };
        if map.is_empty() {
            return full_url;
        }
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (key, value) in map {
            if value.is_null() {
                continue;
            }
            let value_str = value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string());
            pairs.push((key.clone(), value_str));
        }
        if !pairs.is_empty() {
            let query_string = pairs
                .iter()
                .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
                .collect::<Vec<_>>()
                .join("&");
            let separator = if full_url.contains('?') { "&" } else { "?" };
            full_url.push_str(separator);
            full_url.push_str(&query_string);
        }
    }

    full_url
}

fn urlencode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

impl Default for RestExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for RestExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        _context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        if let Some(ref cb) = self.circuit_breaker {
            if !cb.is_allowed() {
                return Ok(BaseExecutor::build_result(
                    false,
                    None,
                    Some("Circuit breaker is open".into()),
                    start.elapsed().as_millis() as i64,
                    0,
                ));
            }
        }

        let config = Self::parse_config(tool);
        let spec = match Self::resolve_spec(tool, parameters, options, &config) {
            Ok(spec) => spec,
            Err(e) => {
                let e = self.apply_error_interceptors(e);
                return Err(e);
            }
        };

        let max_retries = config.max_retries.unwrap_or(0);
        let retry_delay = config.retry_delay.unwrap_or(1000);

        let mut retry_count = 0u32;
        loop {
            let outcome = self
                .execute_once(tool, &spec)
                .await
                .map_err(|e| self.apply_error_interceptors(e));

            let should_retry = match &outcome {
                Ok(result) => {
                    // Failed HTTP responses carry the classification in the
                    // error message; keep a coarse signal here for retries.
                    if result.success {
                        false
                    } else {
                        result
                            .error
                            .as_deref()
                            .map(|e| e.contains("[retryable]"))
                            .unwrap_or(false)
                    }
                }
                Err(e) => {
                    let kind = classify_error(e);
                    kind.is_retryable()
                }
            };

            if !should_retry || retry_count >= max_retries {
                return match outcome {
                    Ok(result) => Ok(ToolExecutionResult {
                        retry_count,
                        ..result
                    }),
                    Err(e) => Err(e),
                };
            }

            retry_count += 1;
            let delay = retry_delay.saturating_mul(2u64.saturating_pow(retry_count - 1));
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }

    fn executor_type(&self) -> &str {
        "rest"
    }
}

fn classify_error(e: &ToolError) -> RestErrorKind {
    match e {
        ToolError::HttpError(_) => RestErrorKind::NetworkError,
        ToolError::Timeout { .. } => RestErrorKind::NetworkError,
        ToolError::RestError { status, .. } => match *status {
            429 => RestErrorKind::RateLimited,
            500..=599 => RestErrorKind::ServerError,
            _ => RestErrorKind::Unknown,
        },
        _ => RestErrorKind::Unknown,
    }
}

impl RestExecutor {
    /// Execute a single request attempt (no retries).
    async fn execute_once(
        &self,
        tool: &wf_types::tool::Tool,
        spec: &RestRequestSpec,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();

        let mut request = self.build_request(tool, spec)?;
        request = self.apply_request_interceptors(request);

        let send_result =
            tokio::time::timeout(Duration::from_millis(spec.timeout_ms), request.send()).await;

        let execution_time = start.elapsed().as_millis() as i64;

        match send_result {
            Ok(Ok(resp)) => {
                let resp = self.apply_response_interceptors(resp);
                let status = resp.status();
                if status.is_success() {
                    let body = resp.json::<Value>().await.unwrap_or(Value::Null);
                    if let Some(ref cb) = self.circuit_breaker {
                        cb.record_success();
                    }
                    Ok(BaseExecutor::build_result(
                        true,
                        Some(serde_json::json!({
                            "status": status.as_u16(),
                            "data": body,
                            "url": spec.url,
                            "method": spec.method,
                        })),
                        None,
                        execution_time,
                        0,
                    ))
                } else {
                    let error_text = resp.text().await.unwrap_or_default();
                    if let Some(ref cb) = self.circuit_breaker {
                        cb.record_failure();
                    }
                    let kind = classify_status(status);
                    let retryable = kind.is_retryable();
                    let message = format!(
                        "REST request to {} failed with status {} ({}): {}{}",
                        spec.url,
                        status.as_u16(),
                        kind.as_str(),
                        if error_text.is_empty() {
                            "".to_string()
                        } else {
                            format!(" | {}", error_text)
                        },
                        if retryable { " [retryable]" } else { "" },
                    );
                    Ok(BaseExecutor::build_result(
                        false,
                        None,
                        Some(message),
                        execution_time,
                        0,
                    ))
                }
            }
            Ok(Err(e)) => {
                if let Some(ref cb) = self.circuit_breaker {
                    cb.record_failure();
                }
                Err(ToolError::HttpError(e))
            }
            Err(_) => {
                if let Some(ref cb) = self.circuit_breaker {
                    cb.record_failure();
                }
                Ok(BaseExecutor::build_result(
                    false,
                    None,
                    Some(format!(
                        "Request to {} timed out after {}ms [retryable]",
                        spec.url, spec.timeout_ms
                    )),
                    execution_time,
                    0,
                ))
            }
        }
    }

    fn build_request(
        &self,
        tool: &wf_types::tool::Tool,
        spec: &RestRequestSpec,
    ) -> ToolResult<RequestBuilder> {
        let mut request = match spec.method.as_str() {
            "GET" => self.client.get(&spec.url),
            "POST" => self.client.post(&spec.url),
            "PUT" => self.client.put(&spec.url),
            "DELETE" => self.client.delete(&spec.url),
            "PATCH" => self.client.patch(&spec.url),
            "HEAD" => self.client.head(&spec.url),
            "OPTIONS" => self.client.request(reqwest::Method::OPTIONS, &spec.url),
            other => {
                return Err(ToolError::ValidationFailed(format!(
                    "Unsupported HTTP method for tool '{}': {}",
                    tool.name, other
                )))
            }
        };

        if let Some(headers) = &spec.headers {
            for (key, value) in headers {
                if let Some(val_str) = value.as_str() {
                    request = request.header(key, val_str);
                }
            }
        }

        if let Some(body) = &spec.body {
            request = request.json(body);
        }

        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::mpsc;

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

    /// Minimal mock HTTP server: accepts one connection, records the raw
    /// request head, and responds with the given body/status.
    fn mock_server(
        response_body: &'static str,
        status: &'static str,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&chunk[..n]);
                            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let _ = tx.send(head);
                let resp = format!(
                    "HTTP/1.1 {} \r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        });
        (addr.to_string(), rx)
    }

    #[tokio::test]
    async fn test_get_method_dispatch_and_url() {
        let (addr, rx) = mock_server(r#"{"ok":true}"#, "200 OK");
        let tool = make_rest_tool(
            "rest_get",
            serde_json::json!({ "base_url": format!("http://{}", addr) }),
        );
        let executor = RestExecutor::new();
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
        let executor = RestExecutor::new();
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
        let mut executor = RestExecutor::new();
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
                    let resp = "HTTP/1.1 500 Server Error \r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
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
        let executor = RestExecutor::new();
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
        let executor = RestExecutor::new();
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

    use std::io::Write;
}

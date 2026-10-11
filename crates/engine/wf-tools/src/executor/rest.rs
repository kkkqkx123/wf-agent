use async_trait::async_trait;
use reqwest::RequestBuilder;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::runtime_config::RestToolConfig;
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;
use wf_types::Metadata;

pub use crate::executor::circuit_breaker::CircuitBreaker;

pub type RequestInterceptor = Arc<dyn Fn(RequestBuilder) -> RequestBuilder + Send + Sync>;
pub type ResponseInterceptor = Arc<dyn Fn(reqwest::Response) -> reqwest::Response + Send + Sync>;
pub type ErrorInterceptor = Arc<dyn Fn(ToolError) -> ToolError + Send + Sync>;

/// Typed classification of a non-2xx REST response.
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

pub struct RestExecutor {
    client: reqwest::Client,
    circuit_breaker: Option<CircuitBreaker>,
    request_interceptors: Vec<RequestInterceptor>,
    response_interceptors: Vec<ResponseInterceptor>,
    error_interceptors: Vec<ErrorInterceptor>,
}

/// Client-level ceiling for the shared HTTP client. Per-call timeouts are
/// enforced separately per request spec; this only bounds body reads the
/// per-attempt timeout does not cover.
const DEFAULT_REST_CLIENT_TIMEOUT_MS: u64 = 60_000;

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
    pub fn new() -> ToolResult<Self> {
        let client = wf_integration::http_client(DEFAULT_REST_CLIENT_TIMEOUT_MS)
            .map_err(ToolError::Internal)?;
        Ok(Self {
            client,
            circuit_breaker: None,
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
            error_interceptors: Vec::new(),
        })
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

    /// Resolve the request spec from the tool config and the call parameters:
    ///
    /// - url: `parameters.url` / `parameters.endpoint`, falling back to
    ///   `{base_url}/{tool.name}`;
    /// - method: `parameters.method` (uppercased), falling back to the tool
    ///   config `method`, then `POST` (the default is `GET`, but the previous
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
/// string parameters.
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

        // Retry policy: per-call options win, config fills the defaults.
        let max_retries = options.retries.or(config.max_retries).unwrap_or(0);
        let retry_delay = options.retry_delay.or(config.retry_delay).unwrap_or(1000);
        let exponential_backoff = options.exponential_backoff.unwrap_or(true);

        let mut retry_count = 0u32;
        loop {
            let outcome = self
                .execute_once(tool, &spec)
                .await
                .map_err(|e| self.apply_error_interceptors(e));

            let should_retry = match &outcome {
                Ok(result) => {
                    // Failed HTTP responses carry their retry hint as a
                    // structured field; the message text stays human-only.
                    if result.success {
                        false
                    } else {
                        result.retryable.unwrap_or(false)
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
            let delay = if exponential_backoff {
                retry_delay.saturating_mul(2u64.saturating_pow(retry_count - 1))
            } else {
                retry_delay
            };
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
                // The headers arrived within `timeout_ms`, but the body must
                // not be allowed to drip forever: bound the read by the same
                // timeout instead of relying only on the reqwest client-level
                // (60s) ceiling.
                let body_result =
                    tokio::time::timeout(Duration::from_millis(spec.timeout_ms), async {
                        if status.is_success() {
                            Ok(resp.json::<Value>().await.unwrap_or(Value::Null))
                        } else {
                            Err(resp.text().await.unwrap_or_default())
                        }
                    })
                    .await;

                match body_result {
                    Ok(Ok(body)) => {
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
                    }
                    Ok(Err(error_text)) => {
                        if let Some(ref cb) = self.circuit_breaker {
                            cb.record_failure();
                        }
                        let kind = classify_status(status);
                        let message = format!(
                            "REST request to {} failed with status {} ({}): {}",
                            spec.url,
                            status.as_u16(),
                            kind.as_str(),
                            if error_text.is_empty() {
                                "".to_string()
                            } else {
                                format!(" | {}", error_text)
                            },
                        );
                        Ok(ToolExecutionResult {
                            retryable: Some(kind.is_retryable()),
                            ..BaseExecutor::build_result(
                                false,
                                None,
                                Some(message),
                                execution_time,
                                0,
                            )
                        })
                    }
                    Err(_) => {
                        if let Some(ref cb) = self.circuit_breaker {
                            cb.record_failure();
                        }
                        Err(ToolError::Timeout {
                            tool_id: spec.url.clone(),
                            timeout_ms: spec.timeout_ms,
                        })
                    }
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
                Ok(ToolExecutionResult {
                    retryable: Some(true),
                    ..BaseExecutor::build_result(
                        false,
                        None,
                        Some(format!(
                            "Request to {} timed out after {}ms",
                            spec.url, spec.timeout_ms
                        )),
                        execution_time,
                        0,
                    )
                })
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

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

pub type RequestInterceptor = Arc<dyn Fn(RequestBuilder) -> RequestBuilder + Send + Sync>;
pub type ResponseInterceptor = Arc<dyn Fn(reqwest::Response) -> reqwest::Response + Send + Sync>;

const CIRCUIT_CLOSED: u8 = 0;
const CIRCUIT_OPEN: u8 = 1;
const CIRCUIT_HALF_OPEN: u8 = 2;

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
        }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self {
            client,
            circuit_breaker: None,
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
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
        }
    }
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
        let base_url = config.base_url.ok_or_else(|| {
            ToolError::ValidationFailed("REST tool requires base_url in config".into())
        })?;

        let url = format!("{}/{}", base_url.trim_end_matches('/'), tool.name);
        let timeout_ms = options.timeout.or(config.timeout).unwrap_or(30000);

        let mut request = self.client.post(&url).json(parameters);

        if let Some(headers) = &config.headers {
            for (key, value) in headers {
                if let Some(val_str) = value.as_str() {
                    request = request.header(key, val_str);
                }
            }
        }

        request = self.apply_request_interceptors(request);

        let response =
            tokio::time::timeout(Duration::from_millis(timeout_ms), request.send()).await;

        let execution_time = start.elapsed().as_millis() as i64;

        let result = match response {
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
                        Some(body),
                        None,
                        execution_time,
                        0,
                    ))
                } else {
                    let error_text = resp.text().await.unwrap_or_default();
                    if let Some(ref cb) = self.circuit_breaker {
                        cb.record_failure();
                    }
                    Ok(BaseExecutor::build_result(
                        false,
                        None,
                        Some(error_text),
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
                    Some(format!("Request timed out after {}ms", timeout_ms)),
                    execution_time,
                    0,
                ))
            }
        };

        result
    }

    fn executor_type(&self) -> &str {
        "rest"
    }
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpRequestOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<HttpMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpResponse<T = serde_json::Value> {
    pub data: T,
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpClientConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_delay: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_circuit_breaker: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_rate_limiter: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circuit_breaker_failure_threshold: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter_capacity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiter_refill_rate: Option<u32>,
}

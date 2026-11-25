//! Network tool configuration: validation and defaults for search backends.
//!
//! The configuration owns routes and declarative request/response shapes.
//! Secrets and proxy credentials are never part of this domain: routes are
//! route-only, header secrets arrive through environment expansion and proxy
//! credentials are resolved by the host at bootstrap.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ConfigError, ConfigResult};

const DEFAULT_TIMEOUT_MS: u64 = 20_000;
const MAX_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MAX_CONTENT_BYTES: usize = 2_000_000;
const MAX_FETCH_CONTENT_BYTES: usize = 100_000_000;
const DEFAULT_MAX_REDIRECTS: usize = 10;
const MAX_REDIRECTS: usize = 20;
const DEFAULT_MAX_RESULTS: usize = 5;
const DEFAULT_RETRY_DELAY_MS: u64 = 100;
const MAX_RETRIES: usize = 3;
const MAX_RATE_INTERVAL_MS: u64 = 60_000;
const MAX_CACHE_TTL_MS: u64 = 3_600_000;
const DEFAULT_FETCH_CACHE_TTL_MS: u64 = 60_000;
const DEFAULT_FETCH_CACHE_CAPACITY: usize = 256;
const MAX_FETCH_CACHE_CAPACITY: usize = 10_000;
const DEFAULT_FETCH_MAX_RETRIES: usize = 2;
const MAX_FETCH_RETRIES: usize = 5;
const DEFAULT_FETCH_RETRY_BASE_MS: u64 = 200;
const DEFAULT_FETCH_RETRY_JITTER_MS: u64 = 100;
const MAX_FETCH_RETRY_MS: u64 = 30_000;
const MAX_RESULTS_CAP: usize = 50;
const PROXY_SCHEMES: [&str; 6] = ["http", "https", "socks4", "socks4a", "socks5", "socks5h"];

fn default_enabled() -> bool {
    true
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_method() -> HttpMethod {
    HttpMethod::Get
}

fn default_max_content_bytes() -> usize {
    DEFAULT_MAX_CONTENT_BYTES
}

fn default_user_agent() -> String {
    "wf-agent/0.1 (Modular Agent Framework)".into()
}

fn default_max_redirects() -> usize {
    DEFAULT_MAX_REDIRECTS
}

fn default_fetch_cache_ttl_ms() -> u64 {
    DEFAULT_FETCH_CACHE_TTL_MS
}

fn default_fetch_cache_capacity() -> usize {
    DEFAULT_FETCH_CACHE_CAPACITY
}

fn default_fetch_max_retries() -> usize {
    DEFAULT_FETCH_MAX_RETRIES
}

fn default_fetch_retry_base_ms() -> u64 {
    DEFAULT_FETCH_RETRY_BASE_MS
}

fn default_fetch_retry_jitter_ms() -> u64 {
    DEFAULT_FETCH_RETRY_JITTER_MS
}

fn default_max_results() -> usize {
    DEFAULT_MAX_RESULTS
}

fn default_retry_delay_ms() -> u64 {
    DEFAULT_RETRY_DELAY_MS
}

fn default_format() -> HttpResponseFormat {
    HttpResponseFormat::Json
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AggregationMode {
    #[default]
    All,
    PriorityFirstSuccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HttpResponseFormat {
    #[default]
    Json,
    DuckduckgoHtml,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    /// Route-only proxy address; userinfo is rejected.
    pub url: String,
    #[serde(default)]
    pub no_proxy: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpRequestConfig {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: HttpMethod,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    /// JSON body with `$query` / `$max_results` placeholders.
    #[serde(default)]
    pub json_body: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultFieldMap {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpResponseConfig {
    #[serde(default = "default_format")]
    pub format: HttpResponseFormat,
    /// Dot path to the result array, with numeric `[n]` segments.
    #[serde(default)]
    pub items_path: Option<String>,
    #[serde(default)]
    pub map: Option<ResultFieldMap>,
    #[serde(default)]
    pub url_prefix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WebSearchBackendConfig {
    DuckduckgoHtml {
        id: String,
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
        #[serde(default = "default_max_redirects")]
        max_redirects: usize,
        #[serde(default)]
        proxy: Option<ProxyConfig>,
        #[serde(default)]
        endpoint: Option<String>,
        #[serde(default)]
        user_agent: Option<String>,
    },
    Http {
        id: String,
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_timeout_ms")]
        timeout_ms: u64,
        #[serde(default = "default_max_redirects")]
        max_redirects: usize,
        #[serde(default)]
        proxy: Option<ProxyConfig>,
        request: Box<HttpRequestConfig>,
        response: Box<HttpResponseConfig>,
    },
}

impl WebSearchBackendConfig {
    pub fn id(&self) -> &str {
        match self {
            Self::DuckduckgoHtml { id, .. } | Self::Http { id, .. } => id,
        }
    }

    pub fn is_enabled(&self) -> bool {
        match self {
            Self::DuckduckgoHtml { enabled, .. } | Self::Http { enabled, .. } => *enabled,
        }
    }

    pub fn timeout_ms(&self) -> u64 {
        match self {
            Self::DuckduckgoHtml { timeout_ms, .. } | Self::Http { timeout_ms, .. } => *timeout_ms,
        }
    }

    pub fn max_redirects(&self) -> usize {
        match self {
            Self::DuckduckgoHtml { max_redirects, .. } | Self::Http { max_redirects, .. } => {
                *max_redirects
            }
        }
    }

    pub fn proxy(&self) -> Option<&ProxyConfig> {
        match self {
            Self::DuckduckgoHtml { proxy, .. } | Self::Http { proxy, .. } => proxy.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebSearchConfig {
    /// Backend ids used when the tool call omits `backend`. `None` uses all.
    #[serde(default)]
    pub default_backends: Option<Vec<String>>,
    #[serde(default)]
    pub mode: AggregationMode,
    /// Hard bound for the whole aggregation, in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    #[serde(default)]
    pub max_retries: usize,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    #[serde(default)]
    pub cache_ttl_ms: u64,
    #[serde(default)]
    pub min_interval_ms: u64,
    #[serde(default)]
    pub backends: Vec<WebSearchBackendConfig>,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            default_backends: None,
            mode: AggregationMode::All,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_results: DEFAULT_MAX_RESULTS,
            max_retries: 0,
            retry_delay_ms: DEFAULT_RETRY_DELAY_MS,
            cache_ttl_ms: 0,
            min_interval_ms: 0,
            backends: vec![WebSearchBackendConfig::DuckduckgoHtml {
                id: "duckduckgo".into(),
                enabled: true,
                timeout_ms: DEFAULT_TIMEOUT_MS,
                max_redirects: DEFAULT_MAX_REDIRECTS,
                proxy: None,
                endpoint: None,
                user_agent: None,
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebFetchConfig {
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_content_bytes")]
    pub max_content_bytes: usize,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: usize,
    /// Fetch backend id. `http` is the built-in default; plugins may add ids.
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
    #[serde(default)]
    pub respect_robots_txt: bool,
    #[serde(default = "default_fetch_cache_ttl_ms")]
    pub cache_ttl_ms: u64,
    #[serde(default = "default_fetch_cache_capacity")]
    pub cache_capacity: usize,
    #[serde(default)]
    pub min_interval_ms: u64,
    /// Backend ids accepted by the `web_fetch` `backend` parameter.
    #[serde(default)]
    pub allowed_backends: Option<Vec<String>>,
    #[serde(default = "default_fetch_max_retries")]
    pub max_retries: usize,
    #[serde(default = "default_fetch_retry_base_ms")]
    pub retry_base_ms: u64,
    #[serde(default = "default_fetch_retry_jitter_ms")]
    pub retry_jitter_ms: u64,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_content_bytes: DEFAULT_MAX_CONTENT_BYTES,
            user_agent: default_user_agent(),
            max_redirects: DEFAULT_MAX_REDIRECTS,
            backend: None,
            proxy: None,
            respect_robots_txt: false,
            cache_ttl_ms: DEFAULT_FETCH_CACHE_TTL_MS,
            cache_capacity: DEFAULT_FETCH_CACHE_CAPACITY,
            min_interval_ms: 0,
            allowed_backends: None,
            max_retries: DEFAULT_FETCH_MAX_RETRIES,
            retry_base_ms: DEFAULT_FETCH_RETRY_BASE_MS,
            retry_jitter_ms: DEFAULT_FETCH_RETRY_JITTER_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct NetworkConfig {
    #[serde(default)]
    pub web_search: WebSearchConfig,
    #[serde(default)]
    pub web_fetch: WebFetchConfig,
}

fn validate_placeholders_value(value: &Value, field: &str) -> ConfigResult<()> {
    match value {
        Value::String(text) => validate_placeholders(text, field),
        Value::Array(items) => {
            for item in items {
                validate_placeholders_value(item, field)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, item) in map {
                validate_placeholders_value(item, &format!("{field}.{key}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_placeholders(text: &str, field: &str) -> ConfigResult<()> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end == start {
            index += 1;
            continue;
        }
        let name = &text[start..end];
        if !matches!(
            name,
            "query" | "max_results" | "language" | "region" | "freshness" | "page" | "offset"
        ) {
            return Err(ConfigError::Validation(format!(
                "unknown placeholder '${name}' in {field}"
            )));
        }
        index = end;
    }
    Ok(())
}

fn has_userinfo(url: &str) -> bool {
    let Some((_, rest)) = url.trim().split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority.contains('@')
}

fn validate_proxy(proxy: &ProxyConfig, backend_id: &str) -> ConfigResult<()> {
    let trimmed = proxy.url.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return Err(ConfigError::Validation(format!(
            "invalid proxy URL for backend '{backend_id}': missing scheme"
        )));
    };
    if !PROXY_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) {
        return Err(ConfigError::Validation(format!(
            "unsupported proxy scheme for backend '{backend_id}'"
        )));
    }
    if rest.split(['/', '?', '#']).next().unwrap_or("").is_empty() || has_userinfo(trimmed) {
        return Err(ConfigError::Validation(format!(
            "proxy route for backend '{backend_id}' must be route-only"
        )));
    }
    Ok(())
}

fn validate_backend(backend: &WebSearchBackendConfig) -> ConfigResult<()> {
    let id = backend.id();
    if id.trim().is_empty() {
        return Err(ConfigError::Validation(
            "search backend id must not be empty".into(),
        ));
    }
    if backend.timeout_ms() == 0 || backend.timeout_ms() > MAX_TIMEOUT_MS {
        return Err(ConfigError::Validation(format!(
            "timeout_ms for backend '{id}' must be between 1 and {MAX_TIMEOUT_MS}"
        )));
    }
    if backend.max_redirects() > MAX_REDIRECTS {
        return Err(ConfigError::Validation(format!(
            "max_redirects for backend '{id}' must not exceed {MAX_REDIRECTS}"
        )));
    }
    if let Some(proxy) = backend.proxy() {
        validate_proxy(proxy, id)?;
    }
    match backend {
        WebSearchBackendConfig::DuckduckgoHtml { endpoint, .. } => {
            if let Some(endpoint) = endpoint {
                if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                    return Err(ConfigError::Validation(format!(
                        "invalid search endpoint for backend '{id}'"
                    )));
                }
            }
        }
        WebSearchBackendConfig::Http {
            request, response, ..
        } => {
            if !request.url.starts_with("http://") && !request.url.starts_with("https://") {
                return Err(ConfigError::Validation(format!(
                    "invalid request URL for backend '{id}'"
                )));
            }
            if request.method == HttpMethod::Get && request.json_body.is_some() {
                return Err(ConfigError::Validation(format!(
                    "GET backend '{id}' cannot declare json_body"
                )));
            }
            for (name, value) in &request.headers {
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                {
                    return Err(ConfigError::Validation(format!(
                        "invalid header name '{name}' for backend '{id}'"
                    )));
                }
                validate_placeholders(value, &format!("backend '{id}' header '{name}'"))?;
            }
            for (name, value) in &request.query {
                validate_placeholders(value, &format!("backend '{id}' query '{name}'"))?;
            }
            if let Some(body) = &request.json_body {
                validate_placeholders_value(body, &format!("backend '{id}' json_body"))?;
            }
            match response.format {
                HttpResponseFormat::Json => {
                    if response.items_path.as_deref().unwrap_or("").is_empty()
                        || response.map.is_none()
                    {
                        return Err(ConfigError::Validation(format!(
                            "JSON backend '{id}' requires items_path and map"
                        )));
                    }
                }
                HttpResponseFormat::DuckduckgoHtml => {}
            }
        }
    }
    Ok(())
}

fn validate_fetch_config(fetch: &WebFetchConfig) -> ConfigResult<()> {
    if fetch.timeout_ms == 0 || fetch.timeout_ms > MAX_TIMEOUT_MS {
        return Err(ConfigError::Validation(format!(
            "web_fetch timeout_ms must be between 1 and {MAX_TIMEOUT_MS}"
        )));
    }
    if fetch.max_content_bytes == 0 || fetch.max_content_bytes > MAX_FETCH_CONTENT_BYTES {
        return Err(ConfigError::Validation(format!(
            "web_fetch max_content_bytes must be between 1 and {MAX_FETCH_CONTENT_BYTES}"
        )));
    }
    if fetch.user_agent.trim().is_empty() {
        return Err(ConfigError::Validation(
            "web_fetch user_agent must not be empty".into(),
        ));
    }
    if fetch
        .backend
        .as_deref()
        .is_some_and(|backend| backend.trim().is_empty())
    {
        return Err(ConfigError::Validation(
            "web_fetch backend id must not be empty".into(),
        ));
    }
    if fetch.max_redirects > MAX_REDIRECTS {
        return Err(ConfigError::Validation(format!(
            "web_fetch max_redirects must not exceed {MAX_REDIRECTS}"
        )));
    }
    if fetch.cache_ttl_ms > MAX_CACHE_TTL_MS {
        return Err(ConfigError::Validation(format!(
            "web_fetch cache_ttl_ms must not exceed {MAX_CACHE_TTL_MS}"
        )));
    }
    if fetch.cache_capacity == 0 || fetch.cache_capacity > MAX_FETCH_CACHE_CAPACITY {
        return Err(ConfigError::Validation(format!(
            "web_fetch cache_capacity must be between 1 and {MAX_FETCH_CACHE_CAPACITY}"
        )));
    }
    if fetch.min_interval_ms > MAX_RATE_INTERVAL_MS {
        return Err(ConfigError::Validation(format!(
            "web_fetch min_interval_ms must not exceed {MAX_RATE_INTERVAL_MS}"
        )));
    }
    if fetch.max_retries > MAX_FETCH_RETRIES {
        return Err(ConfigError::Validation(format!(
            "web_fetch max_retries must not exceed {MAX_FETCH_RETRIES}"
        )));
    }
    if fetch.retry_base_ms.max(fetch.retry_jitter_ms) > MAX_FETCH_RETRY_MS {
        return Err(ConfigError::Validation(format!(
            "web_fetch retry delays must not exceed {MAX_FETCH_RETRY_MS}"
        )));
    }
    if let Some(allowed) = &fetch.allowed_backends {
        if allowed.iter().any(|id| id.trim().is_empty()) {
            return Err(ConfigError::Validation(
                "web_fetch allowed_backends must not contain empty ids".into(),
            ));
        }
        if let Some(selected) = &fetch.backend {
            if !allowed.is_empty() && !allowed.iter().any(|id| id == selected) {
                return Err(ConfigError::Validation(format!(
                    "web_fetch backend '{selected}' is not in allowed_backends"
                )));
            }
        }
    }
    if let Some(proxy) = &fetch.proxy {
        validate_proxy(proxy, "web_fetch")?;
    }
    Ok(())
}

/// Validate the resolved configuration. Unknown backend ids in the default
/// list fail instead of silently widening the backend set.
pub fn validate_network_config(config: &NetworkConfig) -> ConfigResult<()> {
    validate_fetch_config(&config.web_fetch)?;
    let search = &config.web_search;
    if search.timeout_ms == 0 || search.timeout_ms > MAX_TIMEOUT_MS {
        return Err(ConfigError::Validation(format!(
            "web_search timeout_ms must be between 1 and {MAX_TIMEOUT_MS}"
        )));
    }
    if search.max_retries > MAX_RETRIES {
        return Err(ConfigError::Validation(format!(
            "web_search max_retries must not exceed {MAX_RETRIES}"
        )));
    }
    if search.min_interval_ms > MAX_RATE_INTERVAL_MS {
        return Err(ConfigError::Validation(format!(
            "web_search min_interval_ms must not exceed {MAX_RATE_INTERVAL_MS}"
        )));
    }
    if search.cache_ttl_ms > MAX_CACHE_TTL_MS {
        return Err(ConfigError::Validation(format!(
            "web_search cache_ttl_ms must not exceed {MAX_CACHE_TTL_MS}"
        )));
    }
    if search.max_results == 0 || search.max_results > max_results_cap() {
        return Err(ConfigError::Validation(format!(
            "web_search max_results must be between 1 and {}",
            max_results_cap()
        )));
    }
    let mut ids = HashSet::new();
    for backend in &search.backends {
        if !ids.insert(backend.id()) {
            return Err(ConfigError::Validation(format!(
                "duplicate search backend id '{}'",
                backend.id()
            )));
        }
        validate_backend(backend)?;
    }
    if let Some(defaults) = &search.default_backends {
        for id in defaults {
            if !ids.contains(id.as_str()) {
                return Err(ConfigError::Validation(format!(
                    "unknown default search backend '{id}'"
                )));
            }
        }
    }
    Ok(())
}

/// Apply defaults and validate a network configuration.
pub fn transform_network_config(config: NetworkConfig) -> ConfigResult<NetworkConfig> {
    validate_network_config(&config)?;
    Ok(config)
}

/// Exposed for result limiting so config and tool agree on one bound.
pub const fn max_results_cap() -> usize {
    MAX_RESULTS_CAP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_duckduckgo() {
        let config = NetworkConfig::default();
        validate_network_config(&config).expect("default config validates");
        assert_eq!(config.web_search.backends.len(), 1);
        assert_eq!(config.web_search.backends[0].id(), "duckduckgo");
    }

    #[test]
    fn duplicate_backend_ids_fail() {
        let mut config = NetworkConfig::default();
        config
            .web_search
            .backends
            .push(config.web_search.backends[0].clone());
        assert!(validate_network_config(&config).is_err());
    }

    #[test]
    fn proxy_userinfo_is_rejected() {
        let mut config = NetworkConfig::default();
        let WebSearchBackendConfig::DuckduckgoHtml { proxy, .. } =
            &mut config.web_search.backends[0]
        else {
            panic!("default backend is duckduckgo");
        };
        *proxy = Some(ProxyConfig {
            url: "http://user:secret@proxy.example.com:3128".into(),
            no_proxy: Vec::new(),
        });
        assert!(validate_network_config(&config).is_err());
    }

    #[test]
    fn unknown_placeholder_is_rejected() {
        assert!(validate_placeholders("https://x/?q=$query&k=$secret", "query").is_err());
        assert!(validate_placeholders("https://x/?q=$query&n=$max_results", "query").is_ok());
    }

    #[test]
    fn unknown_backend_kind_is_rejected() {
        let config = serde_json::json!({
            "web_search": {
                "backends": [{ "id": "x", "kind": "unknown" }]
            }
        });
        assert!(serde_json::from_value::<NetworkConfig>(config).is_err());
    }

    #[test]
    fn json_backend_requires_response_mapping() {
        let mut config = NetworkConfig::default();
        config.web_search.backends = vec![WebSearchBackendConfig::Http {
            id: "api".into(),
            enabled: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            proxy: None,
            request: Box::new(HttpRequestConfig {
                url: "https://api.example.com".into(),
                method: HttpMethod::Get,
                headers: BTreeMap::new(),
                query: BTreeMap::new(),
                json_body: None,
            }),
            response: Box::new(HttpResponseConfig {
                format: HttpResponseFormat::Json,
                items_path: Some("results".into()),
                map: None,
                url_prefix: None,
            }),
        }];
        assert!(validate_network_config(&config).is_err());
    }

    #[test]
    fn fetch_config_defaults_are_valid() {
        let config = serde_json::json!({ "web_search": {} });
        let config: NetworkConfig = serde_json::from_value(config).expect("web_fetch defaults");
        assert_eq!(
            config.web_fetch.max_content_bytes,
            DEFAULT_MAX_CONTENT_BYTES
        );
        assert_eq!(config.web_fetch.max_redirects, DEFAULT_MAX_REDIRECTS);
        assert_eq!(config.web_fetch.cache_ttl_ms, DEFAULT_FETCH_CACHE_TTL_MS);
        assert_eq!(
            config.web_fetch.cache_capacity,
            DEFAULT_FETCH_CACHE_CAPACITY
        );
        assert!(!config.web_fetch.respect_robots_txt);
        assert_eq!(config.web_fetch.max_retries, DEFAULT_FETCH_MAX_RETRIES);
        assert_eq!(config.web_fetch.retry_base_ms, DEFAULT_FETCH_RETRY_BASE_MS);
        validate_network_config(&config).expect("default fetch config validates");
    }

    #[test]
    fn fetch_only_section_keeps_default_search_backend() {
        let config = serde_json::json!({ "web_fetch": { "max_redirects": 3 } });
        let config: NetworkConfig = serde_json::from_value(config).expect("fetch-only config");
        assert_eq!(config.web_fetch.max_redirects, 3);
        assert_eq!(config.web_search.backends[0].id(), "duckduckgo");
        validate_network_config(&config).expect("fetch-only config validates");
    }

    #[test]
    fn search_governance_limits_are_validated() {
        let mut config = NetworkConfig::default();
        config.web_search.max_retries = MAX_RETRIES + 1;
        assert!(validate_network_config(&config).is_err());
        config = NetworkConfig::default();
        config.web_search.cache_ttl_ms = MAX_CACHE_TTL_MS + 1;
        assert!(validate_network_config(&config).is_err());
    }

    #[test]
    fn declarative_backend_supports_filter_placeholders() {
        let mut config = NetworkConfig::default();
        config.web_search.backends = vec![WebSearchBackendConfig::Http {
            id: "api".into(),
            enabled: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            proxy: None,
            request: Box::new(HttpRequestConfig {
                url: "https://api.example.com/search?q=$query&l=$language&$page".into(),
                method: HttpMethod::Get,
                headers: BTreeMap::new(),
                query: BTreeMap::new(),
                json_body: None,
            }),
            response: Box::new(HttpResponseConfig {
                format: HttpResponseFormat::DuckduckgoHtml,
                items_path: None,
                map: None,
                url_prefix: None,
            }),
        }];
        validate_network_config(&config).expect("filter placeholders validate");
    }

    #[test]
    fn invalid_fetch_limits_are_rejected() {
        let mut config = NetworkConfig::default();
        config.web_fetch.max_content_bytes = 0;
        assert!(validate_network_config(&config).is_err());
        config = NetworkConfig::default();
        config.web_fetch.max_redirects = MAX_REDIRECTS + 1;
        assert!(validate_network_config(&config).is_err());
        config = NetworkConfig::default();
        config.web_fetch.cache_capacity = 0;
        assert!(validate_network_config(&config).is_err());
        config = NetworkConfig::default();
        config.web_fetch.min_interval_ms = MAX_RATE_INTERVAL_MS + 1;
        assert!(validate_network_config(&config).is_err());
        config = NetworkConfig::default();
        config.web_fetch.proxy = Some(ProxyConfig {
            url: "http://user:secret@proxy.test:3128".into(),
            no_proxy: Vec::new(),
        });
        assert!(validate_network_config(&config).is_err());
        config = NetworkConfig::default();
        config.web_fetch.max_retries = MAX_FETCH_RETRIES + 1;
        assert!(validate_network_config(&config).is_err());
        config = NetworkConfig::default();
        config.web_fetch.backend = Some("plugin.fetch".into());
        config.web_fetch.allowed_backends = Some(vec!["http".into()]);
        assert!(validate_network_config(&config).is_err());
    }

    #[test]
    fn unknown_default_backend_is_rejected() {
        let mut config = NetworkConfig::default();
        config.web_search.default_backends = Some(vec!["missing".into()]);
        assert!(validate_network_config(&config).is_err());
    }
}

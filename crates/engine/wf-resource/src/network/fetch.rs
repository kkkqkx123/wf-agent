//! Built-in HTTP fetch backend, registry and router.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;
use encoding_rs::{Encoding, UTF_8};
use futures::StreamExt;
use moka::sync::Cache;
use regex::Regex;
use reqwest::header::{IF_MODIFIED_SINCE, IF_NONE_MATCH};
use std::sync::LazyLock;
use wf_metrics::ToolMetricsCollector;
use wf_tools::{
    redact_transport_error, FetchBackendInfo, FetchToolConfig, WebFetchBackend, WebFetchError,
    WebFetchQuery, WebFetchResponse,
};

use super::backend::SearchBackendClient;

const BUILTIN_FETCH_ID: &str = "http";
const ROBOTS_CACHE_CAPACITY: u64 = 128;
const ROBOTS_CACHE_TTL: Duration = Duration::from_secs(600);
const ROBOTS_MAX_BYTES: usize = 256 * 1024;

static META_CHARSET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<meta\b[^>]*charset\s*=\s*["']?([a-z0-9_-]+)"#)
        .expect("invariant: fixed charset pattern must compile")
});

struct FetchEntry {
    backend: Arc<dyn WebFetchBackend>,
    owner: String,
}

/// Shared registry of fetch backends with owner-based teardown. The selected
/// id lives here so the router reads it at call time; a plugin bridge holding
/// a clone can switch the active backend at runtime.
#[derive(Clone)]
pub struct FetchBackendRegistry {
    entries: Arc<DashMap<String, FetchEntry>>,
    selected: Arc<RwLock<String>>,
}

impl Default for FetchBackendRegistry {
    fn default() -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            selected: Arc::new(RwLock::new(BUILTIN_FETCH_ID.to_string())),
        }
    }
}

impl FetchBackendRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Id currently used by the router.
    pub fn selected(&self) -> String {
        self.selected
            .read()
            .expect("invariant: selected lock is not poisoned")
            .clone()
    }

    /// Switch the active backend. The target must be registered.
    pub fn set_selected(&self, id: &str) -> Result<(), String> {
        if !self.entries.contains_key(id) {
            return Err(format!("fetch backend '{id}' is not registered"));
        }
        *self
            .selected
            .write()
            .expect("invariant: selected lock is not poisoned") = id.to_string();
        Ok(())
    }

    pub fn register_builtin(&self, backend: Arc<dyn WebFetchBackend>) -> Result<(), String> {
        self.register("builtin", backend)
    }

    pub fn register_plugin(
        &self,
        plugin_id: &str,
        backend: Arc<dyn WebFetchBackend>,
    ) -> Result<(), String> {
        self.register(plugin_id, backend)
    }

    fn register(&self, owner: &str, backend: Arc<dyn WebFetchBackend>) -> Result<(), String> {
        let id = backend.id().to_string();
        match self.entries.entry(id.clone()) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                Err(format!("fetch backend id '{id}' is already registered"))
            }
            dashmap::mapref::entry::Entry::Vacant(slot) => {
                slot.insert(FetchEntry {
                    backend,
                    owner: owner.to_string(),
                });
                Ok(())
            }
        }
    }

    pub fn unregister_by_owner(&self, owner: &str) -> Vec<String> {
        let mut removed = Vec::new();
        self.entries.retain(|id, entry| {
            if entry.owner == owner {
                removed.push(id.clone());
                false
            } else {
                true
            }
        });
        removed.sort();
        let active = self.selected();
        if removed.contains(&active) && self.entries.contains_key(BUILTIN_FETCH_ID) {
            *self
                .selected
                .write()
                .expect("invariant: selected lock is not poisoned") = BUILTIN_FETCH_ID.to_string();
        }
        removed
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .entries
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        ids.sort();
        ids
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn WebFetchBackend>> {
        self.entries.get(id).map(|entry| entry.backend.clone())
    }
}

/// Routes every fetch call to the registry's current selected backend id.
/// The selection is read at call time so it can change at runtime.
pub struct WebFetchRouter {
    registry: FetchBackendRegistry,
    allowed: Option<Vec<String>>,
}

impl WebFetchRouter {
    pub fn new(
        registry: FetchBackendRegistry,
        selected: impl Into<String>,
        allowed: Option<Vec<String>>,
    ) -> Self {
        let router = Self { registry, allowed };
        // Selection is owned by the registry; construction stays infallible
        // so plugin backends can register after bootstrap.
        let _ = router.registry.set_selected(&selected.into());
        router
    }

    fn resolve(&self, requested: Option<&str>) -> Result<Arc<dyn WebFetchBackend>, WebFetchError> {
        let selected = self.registry.selected();
        let id = requested.unwrap_or(selected.as_str());
        if let Some(allowed) = &self.allowed {
            if !allowed.is_empty() && !allowed.iter().any(|candidate| candidate == id) {
                return Err(WebFetchError::BackendNotFound(id.to_string()));
            }
        }
        self.registry
            .get(id)
            .ok_or_else(|| WebFetchError::BackendNotFound(id.to_string()))
    }
}

#[async_trait]
impl WebFetchBackend for WebFetchRouter {
    async fn fetch(&self, mut query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
        let backend = self.resolve(query.backend.as_deref())?;
        query.backend = None;
        backend.fetch(query).await
    }

    fn id(&self) -> &str {
        "route.web_fetch"
    }

    fn kind(&self) -> &str {
        "router"
    }

    fn catalog(&self) -> Vec<FetchBackendInfo> {
        let mut entries: Vec<FetchBackendInfo> = self
            .registry
            .ids()
            .into_iter()
            .filter(|id| {
                self.allowed
                    .as_ref()
                    .is_none_or(|allowed| allowed.is_empty() || allowed.contains(id))
            })
            .flat_map(|id| self.registry.get(&id))
            .flat_map(|backend| backend.catalog())
            .collect();
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        entries.dedup_by(|left, right| left.id == right.id);
        entries
    }
}

#[derive(Clone)]
struct CachedFetch {
    response: WebFetchResponse,
}

#[derive(Clone, Default)]
struct RobotsRules {
    rules: Vec<(Regex, bool)>,
    crawl_delay_ms: Option<u64>,
}

impl RobotsRules {
    fn disallows(&self, path: &str) -> bool {
        let mut allowed = false;
        let mut longest = 0usize;
        for (pattern, is_allowed) in &self.rules {
            if let Some(found) = pattern.find(path) {
                if found.start() == 0 && found.end() >= longest {
                    longest = found.end();
                    allowed = *is_allowed;
                }
            }
        }
        longest > 0 && !allowed
    }
}

/// Default HTTP implementation with streaming size, redirect, cache, robots
/// and domain throttle guards.
pub struct HttpFetchBackend {
    id: String,
    client: reqwest::Client,
    user_agent: String,
    max_content_bytes: usize,
    respect_robots_txt: bool,
    cache_ttl_ms: u64,
    min_interval_ms: u64,
    max_retries: usize,
    retry_base_ms: u64,
    retry_jitter_ms: u64,
    metrics: Option<Arc<ToolMetricsCollector>>,
    cache: Cache<String, CachedFetch>,
    robots: Cache<String, RobotsRules>,
    last_call: DashMap<String, Arc<tokio::sync::Mutex<Instant>>>,
}

impl HttpFetchBackend {
    pub fn new(
        id: impl Into<String>,
        config: &FetchToolConfig,
        metrics: Option<Arc<ToolMetricsCollector>>,
    ) -> Result<Self, String> {
        let client = SearchBackendClient::build(
            config.timeout_ms,
            &config.user_agent,
            &config.proxy,
            &config.domain_policy,
            config.max_redirects,
        )?;
        let cache = Cache::builder()
            .max_capacity(config.cache_capacity.max(1) as u64)
            .time_to_live(Duration::from_millis(config.cache_ttl_ms.max(1)))
            .build();
        Ok(Self {
            id: id.into(),
            client,
            user_agent: config.user_agent.clone(),
            max_content_bytes: config.max_content_bytes,
            respect_robots_txt: config.respect_robots_txt,
            cache_ttl_ms: config.cache_ttl_ms,
            min_interval_ms: config.min_interval_ms,
            max_retries: config.max_retries,
            retry_base_ms: config.retry_base_ms,
            retry_jitter_ms: config.retry_jitter_ms,
            metrics,
            cache,
            robots: Cache::builder()
                .max_capacity(ROBOTS_CACHE_CAPACITY)
                .time_to_live(ROBOTS_CACHE_TTL)
                .build(),
            last_call: DashMap::new(),
        })
    }

    async fn throttle(&self, host: &str, delay_ms: u64) {
        if delay_ms == 0 {
            return;
        }
        let interval = Duration::from_millis(delay_ms);
        let clock = self
            .last_call
            .entry(host.to_string())
            .or_insert_with(|| {
                Arc::new(tokio::sync::Mutex::new(
                    Instant::now()
                        .checked_sub(interval)
                        .unwrap_or_else(Instant::now),
                ))
            })
            .clone();
        let mut last = clock.lock().await;
        let elapsed = last.elapsed();
        if elapsed < interval {
            tokio::time::sleep(interval - elapsed).await;
        }
        *last = Instant::now();
    }

    async fn robots_for(&self, url: &reqwest::Url) -> RobotsRules {
        let origin = url.origin().ascii_serialization().to_string();
        if let Some(rules) = self.robots.get(&origin) {
            return rules;
        }
        let robots_url = format!("{origin}/robots.txt");
        let rules = match self.client.get(&robots_url).send().await {
            Ok(response) if response.status().is_success() => {
                let mut bytes = Vec::new();
                let mut stream = response.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(chunk) if bytes.len() + chunk.len() <= ROBOTS_MAX_BYTES => {
                            bytes.extend_from_slice(&chunk)
                        }
                        Ok(_) => break,
                        Err(_) => break,
                    }
                }
                parse_robots(&String::from_utf8_lossy(&bytes), &self.user_agent)
            }
            _ => RobotsRules::default(),
        };
        self.robots.insert(origin, rules.clone());
        rules
    }

    fn cached_response(&self, key: &str) -> Option<WebFetchResponse> {
        if self.cache_ttl_ms == 0 {
            return None;
        }
        self.cache.get(key).and_then(|entry| {
            let response = entry.response.clone();
            let vary = response
                .headers
                .get("vary")
                .map(String::as_str)
                .unwrap_or("");
            if vary.is_empty() || vary.eq_ignore_ascii_case("accept-encoding") {
                Some(response)
            } else {
                None
            }
        })
    }

    fn store_response(&self, key: &str, response: &WebFetchResponse) {
        if self.cache_ttl_ms > 0 {
            self.cache.insert(
                key.to_string(),
                CachedFetch {
                    response: response.clone(),
                },
            );
        }
    }

    fn record_call(&self, success: bool, elapsed: Duration) {
        if let Some(metrics) = &self.metrics {
            metrics.record_backend_call(&self.id, success, elapsed.as_secs_f64() * 1000.0);
        }
    }

    fn retry_delay(&self, attempt: usize) -> Duration {
        let base = self.retry_base_ms.saturating_mul(1u64 << attempt.min(6));
        let jitter = if self.retry_jitter_ms == 0 {
            0
        } else {
            (Instant::now().elapsed().subsec_nanos() as u64) % self.retry_jitter_ms
        };
        Duration::from_millis(base.saturating_add(jitter))
    }
}

#[async_trait]
impl WebFetchBackend for HttpFetchBackend {
    async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
        if query.render {
            return Err(WebFetchError::UnsupportedOption("render"));
        }
        if query.wait_for_ms.is_some() {
            return Err(WebFetchError::UnsupportedOption("wait_for_ms"));
        }
        let started = Instant::now();
        let parsed = reqwest::Url::parse(&query.url).map_err(|_| WebFetchError::InvalidUrl)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(WebFetchError::UnsupportedScheme);
        }

        let mut robots = RobotsRules::default();
        if let Some(host) = parsed.host_str() {
            if self.respect_robots_txt {
                let path = match parsed.query() {
                    Some(value) => format!("{}?{value}", parsed.path()),
                    None => parsed.path().to_string(),
                };
                robots = self.robots_for(&parsed).await;
                if robots.disallows(&path) {
                    let error = WebFetchError::DomainDenied(format!("robots.txt disallows {path}"));
                    self.record_call(false, started.elapsed());
                    return Err(error);
                }
            }
            let delay = robots.crawl_delay_ms.unwrap_or(0).max(self.min_interval_ms);
            self.throttle(host, delay).await;
        }

        let key = parsed.to_string();
        let mut cached = if query.refresh {
            None
        } else {
            self.cached_response(&key)
        };
        let mut attempt = 0usize;
        loop {
            let mut request = self.client.get(parsed.clone());
            if let Some(cached) = &cached {
                if let Some(etag) = cached.headers.get("etag") {
                    request = request.header(IF_NONE_MATCH, etag);
                }
                if let Some(modified) = cached.headers.get("last-modified") {
                    request = request.header(IF_MODIFIED_SINCE, modified);
                }
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    let error = WebFetchError::Transport(redact_transport_error(error));
                    if attempt >= self.max_retries {
                        self.record_call(false, started.elapsed());
                        return Err(error);
                    }
                    tokio::time::sleep(self.retry_delay(attempt)).await;
                    attempt += 1;
                    continue;
                }
            };
            let status = response.status();
            if status == reqwest::StatusCode::NOT_MODIFIED {
                if let Some(mut response) = cached.take() {
                    response.cache_status = "hit_validated".to_string();
                    response.elapsed_ms = started.elapsed().as_millis() as u64;
                    self.record_call(true, started.elapsed());
                    return Ok(response);
                }
            }
            if !status.is_success() {
                let retry_after_ms = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .map(|seconds| seconds.saturating_mul(1000));
                let code = status.as_u16();
                let error = if matches!(code, 401 | 403 | 429 | 503) {
                    WebFetchError::Blocked(format!("HTTP {code}"))
                } else {
                    WebFetchError::HttpStatus(code)
                };
                let retryable = code == 429 || status.is_server_error();
                if !retryable || attempt >= self.max_retries {
                    self.record_call(false, started.elapsed());
                    return Err(error);
                }
                let delay = retry_after_ms.unwrap_or_else(|| {
                    let millis = self.retry_delay(attempt).as_millis();
                    u64::try_from(millis).unwrap_or(u64::MAX)
                });
                tokio::time::sleep(Duration::from_millis(delay)).await;
                attempt += 1;
                continue;
            }

            let final_url = response.url().to_string();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_string();
            let headers = response
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    let name = name.as_str().to_ascii_lowercase();
                    let keep = matches!(
                        name.as_str(),
                        "content-type"
                            | "etag"
                            | "last-modified"
                            | "content-disposition"
                            | "cache-control"
                            | "vary"
                    );
                    keep.then(|| (name, value.to_str().unwrap_or("").to_string()))
                })
                .collect::<BTreeMap<_, _>>();

            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk
                    .map_err(|error| WebFetchError::Transport(redact_transport_error(error)))?;
                if bytes.len() + chunk.len() > self.max_content_bytes {
                    let error = WebFetchError::TooLarge {
                        size: bytes.len() + chunk.len(),
                        limit: self.max_content_bytes,
                    };
                    self.record_call(false, started.elapsed());
                    return Err(error);
                }
                bytes.extend_from_slice(&chunk);
            }
            let content = decode_body(&bytes, &content_type);
            if let Some(reason) = block_reason(&content) {
                let error = WebFetchError::Blocked(reason);
                self.record_call(false, started.elapsed());
                return Err(error);
            }
            let cache_status = if cached.is_some() {
                "hit_fallback"
            } else {
                "miss"
            };
            let result = WebFetchResponse {
                final_url,
                status_code: status.as_u16(),
                content_type,
                headers,
                content,
                size: bytes.len(),
                cache_status: cache_status.to_string(),
                elapsed_ms: started.elapsed().as_millis() as u64,
                title: None,
                metadata: None,
                links: None,
                tables: None,
                options_applied: false,
            };
            self.store_response(&key, &result);
            self.record_call(true, started.elapsed());
            return Ok(result);
        }
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &str {
        "http"
    }

    fn catalog(&self) -> Vec<FetchBackendInfo> {
        vec![FetchBackendInfo {
            id: self.id.clone(),
            kind: "http".to_string(),
            capabilities: vec![
                "web_fetch".to_string(),
                "selector".to_string(),
                "exclude_selector".to_string(),
                "focus".to_string(),
                "min_words".to_string(),
                "link_mode".to_string(),
                "clean_html".to_string(),
                "cache".to_string(),
                "refresh".to_string(),
            ],
        }]
    }
}

fn block_reason(content: &str) -> Option<String> {
    let head: String = content
        .chars()
        .take(8192)
        .collect::<String>()
        .to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "captcha",
        "access denied",
        "verify you are human",
        "attention required",
        "cf-browser-verification",
    ];
    MARKERS
        .iter()
        .find(|marker| head.contains(**marker))
        .map(|marker| (*marker).to_string())
}

fn decode_body(bytes: &[u8], content_type: &str) -> String {
    let (bom_payload, bom_encoding) = strip_bom(bytes);
    let label = charset_from_content_type(content_type)
        .or_else(|| charset_from_meta(bytes))
        .unwrap_or_default();
    let declared = Encoding::for_label(label.as_bytes());
    let (payload, encoding) = match declared {
        Some(encoding) if Some(encoding) == bom_encoding => (bom_payload, encoding),
        Some(encoding) => (bytes, encoding),
        None => match bom_encoding {
            Some(encoding) => (bom_payload, encoding),
            None => (bytes, UTF_8),
        },
    };
    let (text, _, _) = encoding.decode(payload);
    text.into_owned()
}

fn strip_bom(bytes: &[u8]) -> (&[u8], Option<&'static Encoding>) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (&bytes[3..], Some(UTF_8))
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        (&bytes[2..], Some(encoding_rs::UTF_16LE))
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (&bytes[2..], Some(encoding_rs::UTF_16BE))
    } else {
        (bytes, None)
    }
}

fn charset_from_content_type(content_type: &str) -> Option<String> {
    let (_, parameters) = content_type.split_once(';')?;
    parameters.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key.eq_ignore_ascii_case("charset"))
            .then(|| value.trim().trim_matches(['"', '\'']).to_ascii_lowercase())
    })
}

fn charset_from_meta(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    META_CHARSET_RE
        .captures(&head)
        .map(|captures| captures[1].to_ascii_lowercase())
}

type RobotGroup = (Vec<String>, Vec<(String, bool)>, Option<u64>);

fn push_robot_group(
    groups: &mut Vec<RobotGroup>,
    agents: &mut Vec<String>,
    rules: &mut Vec<(String, bool)>,
    crawl_delay: &mut Option<u64>,
) {
    if !agents.is_empty() {
        groups.push((
            std::mem::take(agents),
            std::mem::take(rules),
            crawl_delay.take(),
        ));
    }
}

fn robot_regex(pattern: &str) -> Option<Regex> {
    let exact = pattern.ends_with('$');
    let body = if exact {
        &pattern[..pattern.len() - 1]
    } else {
        pattern
    };
    let mut source = String::from("^");
    for part in body.split('*') {
        source.push_str(&regex::escape(part));
        source.push_str(".*");
    }
    if exact {
        source.push('$');
    }
    Regex::new(&source).ok()
}

fn parse_robots(text: &str, user_agent: &str) -> RobotsRules {
    let agent_token = user_agent
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let user_agent = user_agent.to_ascii_lowercase();
    let mut groups: Vec<RobotGroup> = Vec::new();
    let mut agents: Vec<String> = Vec::new();
    let mut rules: Vec<(String, bool)> = Vec::new();
    let mut crawl_delay = None;
    let mut reading_agents = true;

    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "user-agent" => {
                if !reading_agents {
                    push_robot_group(&mut groups, &mut agents, &mut rules, &mut crawl_delay);
                    reading_agents = true;
                }
                agents.push(value.trim().to_ascii_lowercase());
            }
            "allow" | "disallow" => {
                reading_agents = false;
                let pattern = value.trim();
                if !pattern.is_empty() {
                    rules.push((
                        pattern.to_string(),
                        key.trim().eq_ignore_ascii_case("allow"),
                    ));
                }
            }
            "crawl-delay" => {
                reading_agents = false;
                crawl_delay = value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|seconds| (seconds.max(0.0) * 1000.0) as u64);
            }
            _ => {}
        }
    }
    push_robot_group(&mut groups, &mut agents, &mut rules, &mut crawl_delay);

    let mut compiled = Vec::new();
    let mut delay = None;
    for (group_agents, group_rules, group_delay) in groups {
        if !group_agents
            .iter()
            .any(|agent| agent == "*" || agent == &user_agent || agent == &agent_token)
        {
            continue;
        }
        for (pattern, allowed) in group_rules {
            if let Some(regex) = robot_regex(&pattern) {
                compiled.push((regex, allowed));
            }
        }
        if group_delay.is_some() {
            delay = group_delay;
        }
    }
    RobotsRules {
        rules: compiled,
        crawl_delay_ms: delay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubFetch {
        id: String,
        tag: String,
    }

    #[async_trait]
    impl WebFetchBackend for StubFetch {
        async fn fetch(&self, _query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
            Ok(WebFetchResponse {
                final_url: "https://example.com".into(),
                status_code: 200,
                content_type: "text/plain".into(),
                headers: BTreeMap::new(),
                content: self.tag.clone(),
                size: self.tag.len(),
                cache_status: "unavailable".into(),
                elapsed_ms: 0,
                title: None,
                metadata: None,
                links: None,
                tables: None,
                options_applied: false,
            })
        }

        fn id(&self) -> &str {
            &self.id
        }

        fn kind(&self) -> &str {
            "stub"
        }
    }

    #[test]
    fn selection_switches_at_runtime_and_rejects_unknown() {
        let registry = FetchBackendRegistry::new();
        registry
            .register_builtin(Arc::new(StubFetch {
                id: "http".into(),
                tag: "builtin".into(),
            }))
            .expect("register builtin");
        registry
            .register_plugin(
                "p1",
                Arc::new(StubFetch {
                    id: "p1.fetch".into(),
                    tag: "plugin".into(),
                }),
            )
            .expect("register plugin");

        let router = WebFetchRouter::new(registry.clone(), "http", None);
        assert_eq!(registry.selected(), "http");
        assert!(registry.set_selected("p1.fetch").is_ok());
        assert_eq!(registry.selected(), "p1.fetch");
        assert!(registry.set_selected("missing").is_err());
        assert_eq!(registry.selected(), "p1.fetch");

        let query = WebFetchQuery::new("https://example.com");
        let routed = futures::executor::block_on(router.fetch(query)).expect("fetch succeeds");
        assert_eq!(routed.content, "plugin");

        let mut explicit = WebFetchQuery::new("https://example.com");
        explicit.backend = Some("http".into());
        let routed = futures::executor::block_on(router.fetch(explicit)).expect("explicit backend");
        assert_eq!(routed.content, "builtin");

        let restricted =
            WebFetchRouter::new(registry.clone(), "http", Some(vec!["http".to_string()]));
        let mut denied = WebFetchQuery::new("https://example.com");
        denied.backend = Some("p1.fetch".into());
        assert!(futures::executor::block_on(restricted.fetch(denied)).is_err());
    }

    #[test]
    fn removing_active_owner_falls_back_to_builtin() {
        let registry = FetchBackendRegistry::new();
        registry
            .register_builtin(Arc::new(StubFetch {
                id: "http".into(),
                tag: "builtin".into(),
            }))
            .expect("register builtin");
        registry
            .register_plugin(
                "p1",
                Arc::new(StubFetch {
                    id: "p1.fetch".into(),
                    tag: "plugin".into(),
                }),
            )
            .expect("register plugin");
        registry.set_selected("p1.fetch").expect("select plugin");

        let removed = registry.unregister_by_owner("p1");
        assert_eq!(removed, vec!["p1.fetch".to_string()]);
        assert_eq!(registry.selected(), "http");
    }

    #[test]
    fn decodes_charset_from_header_and_meta() {
        let label = "\u{4e2d}\u{6587}";
        let gbk = encoding_rs::GBK.encode(label).0;
        let decoded = decode_body(gbk.as_ref(), "text/html; charset=gbk");
        assert_eq!(decoded, label);
        let meta = b"<html><head><meta charset='gbk'></head><body>x</body></html>";
        assert!(charset_from_meta(meta).is_some());
        let with_bom = [&[0xEF, 0xBB, 0xBF][..], b"hello".as_slice()].concat();
        assert_eq!(decode_body(&with_bom, "text/plain"), "hello");
        assert_eq!(decode_body(&with_bom, "text/plain; charset=utf-8"), "hello");
    }

    #[test]
    fn parses_robots_rules_for_matching_agent() {
        let text = "User-agent: otherbot\nUser-agent: wf-agent/0.1\nDisallow: /shared\n\
                    User-agent: *\nDisallow: /private\n\
                    User-agent: wf-agent\nDisallow: /secret\n";
        let rules = parse_robots(text, "wf-agent/0.1");
        assert!(rules.disallows("/shared/x"));
        assert!(rules.disallows("/private/a"));
        assert!(rules.disallows("/secret"));
        assert!(!rules.disallows("/public"));
    }

    #[test]
    fn robots_respects_allow_wildcards_and_crawl_delay() {
        let text = "User-agent: *\nDisallow: /private/*\nAllow: /private/public\n\
                    Crawl-delay: 1.5\n";
        let rules = parse_robots(text, "wf-agent");
        assert!(rules.disallows("/private/notes"));
        assert!(!rules.disallows("/private/public/page"));
        assert_eq!(rules.crawl_delay_ms, Some(1500));
        assert!(robot_regex("/exact$").is_some_and(|regex| regex.is_match("/exact")));
    }

    #[test]
    fn detects_block_markers_in_html_body() {
        assert_eq!(
            block_reason("<html><body>Access Denied</body></html>"),
            Some("access denied".into())
        );
        assert!(block_reason("<html><body>Hello</body></html>").is_none());
    }

    #[tokio::test]
    async fn rejects_wait_option_on_http_backend() {
        let backend = HttpFetchBackend::new("http-test", &FetchToolConfig::default(), None)
            .expect("backend builds");
        let mut query = WebFetchQuery::new("https://example.com");
        query.wait_for_ms = Some(10);
        let error = backend.fetch(query).await.expect_err("wait is unsupported");
        assert!(error.to_string().contains("wait_for_ms"));
    }

    #[tokio::test]
    async fn throttle_serializes_requests_to_one_host() {
        let config = FetchToolConfig {
            min_interval_ms: 60,
            ..Default::default()
        };
        let backend =
            HttpFetchBackend::new("http-throttle", &config, None).expect("backend builds");
        let started = Instant::now();
        futures::future::join_all((0..3).map(|_| backend.throttle("example.com", 60))).await;
        assert!(started.elapsed() >= Duration::from_millis(120));
    }

    fn spawn_conditional_server() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local fetch server");
        let address = listener.local_addr().expect("resolve fetch server address");
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut reader = BufReader::new(&stream);
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) if line == "\r\n" || line == "\n" => break,
                        Ok(_) => request.push_str(&line),
                    }
                }
                counter.fetch_add(1, Ordering::Relaxed);
                let response = if request.to_ascii_lowercase().contains("if-none-match") {
                    "HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nETag: \"v1\"\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://{address}/page"), requests)
    }

    #[tokio::test]
    async fn conditional_requests_reuse_cached_body() {
        let (url, requests) = spawn_conditional_server();
        let config = FetchToolConfig {
            cache_ttl_ms: 60_000,
            cache_capacity: 8,
            ..Default::default()
        };
        let backend = HttpFetchBackend::new("http-test", &config, None).expect("backend builds");
        let first = backend
            .fetch(WebFetchQuery::new(url.clone()))
            .await
            .expect("first fetch succeeds");
        let second = backend
            .fetch(WebFetchQuery::new(url))
            .await
            .expect("conditional fetch succeeds");
        assert_eq!(first.content, "hello");
        assert_eq!(second.content, "hello");
        assert_eq!(second.cache_status, "hit_validated");
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 2);
    }
}

//! Search backend registry and web-domain aggregation.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;
use futures::future::join_all;
use serde_json::json;
use wf_config::processor::network::{max_results_cap, AggregationMode};
use wf_metrics::ToolMetricsCollector;
use wf_tools::{
    SearchBackend, SearchBackendInfo, WebSearchBackend, WebSearchError, WebSearchQuery,
    WebSearchResponse, WebSearchResult,
};

type SelectedBackend = (String, Arc<dyn WebSearchBackend>);

struct BackendEntry {
    backend: Arc<dyn WebSearchBackend>,
    owner: String,
    priority: usize,
}

struct RegistryInner {
    entries: DashMap<String, BackendEntry>,
    sequence: AtomicUsize,
}

/// Shared registry of web search backends. The list is read at call time, so
/// plugins activated after tool registration join aggregation immediately.
#[derive(Clone)]
pub struct NetworkRegistry {
    inner: Arc<RegistryInner>,
}

impl Default for NetworkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for NetworkRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkRegistry")
            .field("backends", &self.ids())
            .finish()
    }
}

impl NetworkRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RegistryInner {
                entries: DashMap::new(),
                sequence: AtomicUsize::new(0),
            }),
        }
    }

    /// Register a built-in backend; ids are owned by the `builtin` owner.
    pub fn register_builtin(&self, backend: Arc<dyn WebSearchBackend>) -> Result<(), String> {
        self.register("builtin", backend)
    }

    /// Register a plugin backend under the plugin id for symmetric teardown.
    pub fn register_plugin(
        &self,
        plugin_id: &str,
        backend: Arc<dyn WebSearchBackend>,
    ) -> Result<(), String> {
        self.register(plugin_id, backend)
    }

    fn register(&self, owner: &str, backend: Arc<dyn WebSearchBackend>) -> Result<(), String> {
        let id = backend.id().to_string();
        let priority = self.inner.sequence.fetch_add(1, Ordering::Relaxed);
        let entry = BackendEntry {
            backend,
            owner: owner.to_string(),
            priority,
        };
        match self.inner.entries.entry(id.clone()) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                Err(format!("search backend id '{id}' is already registered"))
            }
            dashmap::mapref::entry::Entry::Vacant(slot) => {
                slot.insert(entry);
                Ok(())
            }
        }
    }

    /// Remove every backend owned by one plugin. Returns the removed ids.
    pub fn unregister_by_owner(&self, owner: &str) -> Vec<String> {
        let mut removed = Vec::new();
        self.inner.entries.retain(|id, entry| {
            if entry.owner == owner {
                removed.push(id.clone());
                false
            } else {
                true
            }
        });
        removed.sort();
        removed
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .inner
            .entries
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        ids.sort();
        ids
    }

    pub fn has(&self, id: &str) -> bool {
        self.inner.entries.contains_key(id)
    }

    fn snapshot(&self) -> Vec<(String, Arc<dyn WebSearchBackend>, usize)> {
        let mut entries: Vec<(String, Arc<dyn WebSearchBackend>, usize)> = self
            .inner
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.key().clone(),
                    entry.value().backend.clone(),
                    entry.value().priority,
                )
            })
            .collect();
        entries.sort_by(|left, right| left.2.cmp(&right.2).then_with(|| left.0.cmp(&right.0)));
        entries
    }
}

/// Runtime controls for search aggregation.
#[derive(Default)]
pub struct SearchAggregationOptions {
    pub max_retries: usize,
    pub retry_delay_ms: u64,
    pub cache_ttl_ms: u64,
    pub min_interval_ms: u64,
    pub metrics: Option<Arc<ToolMetricsCollector>>,
}

/// Aggregates the registered web backends behind one tool backend.
pub struct WebSearchAggregator {
    registry: NetworkRegistry,
    mode: AggregationMode,
    timeout_ms: u64,
    default_backends: Option<Vec<String>>,
    options: SearchAggregationOptions,
    cache: DashMap<String, (Instant, WebSearchResponse)>,
    last_call: DashMap<String, Instant>,
}

impl WebSearchAggregator {
    pub fn new(
        registry: NetworkRegistry,
        mode: AggregationMode,
        timeout_ms: u64,
        default_backends: Option<Vec<String>>,
    ) -> Self {
        Self::new_with_options(
            registry,
            mode,
            timeout_ms,
            default_backends,
            SearchAggregationOptions::default(),
        )
    }

    pub fn new_with_options(
        registry: NetworkRegistry,
        mode: AggregationMode,
        timeout_ms: u64,
        default_backends: Option<Vec<String>>,
        options: SearchAggregationOptions,
    ) -> Self {
        Self {
            registry,
            mode,
            timeout_ms,
            default_backends,
            options,
            cache: DashMap::new(),
            last_call: DashMap::new(),
        }
    }

    fn selected(&self, requested: Option<&str>) -> Result<Vec<SelectedBackend>, WebSearchError> {
        let entries = self.registry.snapshot();
        if let Some(id) = requested {
            if self
                .default_backends
                .as_ref()
                .is_some_and(|allowed| !allowed.iter().any(|candidate| candidate == id))
            {
                return Err(WebSearchError::BackendNotFound(id.to_string()));
            }
            let entry = entries
                .into_iter()
                .find(|(candidate, _, _)| candidate == id)
                .ok_or_else(|| WebSearchError::BackendNotFound(id.to_string()))?;
            return Ok(vec![(entry.0, entry.1)]);
        }
        let mut selected: Vec<SelectedBackend> = entries
            .into_iter()
            .filter(|(id, _, _)| {
                self.default_backends
                    .as_ref()
                    .is_none_or(|allowed| allowed.contains(id))
            })
            .map(|(id, backend, _)| (id, backend))
            .collect();
        if let Some(order) = &self.default_backends {
            selected.sort_by_key(|(id, _)| {
                order
                    .iter()
                    .position(|candidate| candidate == id)
                    .unwrap_or(usize::MAX)
            });
        }
        if selected.is_empty() {
            return Err(WebSearchError::NoBackend);
        }
        Ok(selected)
    }

    async fn throttle(&self, id: &str) {
        let interval = self.options.min_interval_ms;
        if interval == 0 {
            return;
        }
        let interval = Duration::from_millis(interval);
        let wait = {
            let mut last = self.last_call.entry(id.to_string()).or_insert_with(|| {
                Instant::now()
                    .checked_sub(interval)
                    .unwrap_or_else(Instant::now)
            });
            let elapsed = last.elapsed();
            if elapsed >= interval {
                *last = Instant::now();
                None
            } else {
                Some(interval - elapsed)
            }
        };
        if let Some(wait) = wait {
            tokio::time::sleep(wait).await;
            self.last_call.insert(id.to_string(), Instant::now());
        }
    }

    async fn call(
        &self,
        id: &str,
        backend: &Arc<dyn WebSearchBackend>,
        query: &WebSearchQuery,
    ) -> Result<WebSearchResponse, WebSearchError> {
        self.throttle(id).await;
        let mut attempt = 0;
        loop {
            let started = Instant::now();
            let result = backend
                .search(WebSearchQuery {
                    query: query.query.clone(),
                    max_results: query.max_results,
                    backend: None,
                    language: query.language.clone(),
                    region: query.region.clone(),
                    freshness: query.freshness.clone(),
                    page: query.page,
                })
                .await;
            if let Some(metrics) = &self.options.metrics {
                metrics.record_backend_call(
                    id,
                    result.is_ok(),
                    started.elapsed().as_secs_f64() * 1000.0,
                );
            }
            match result {
                Ok(response) => return Ok(response),
                Err(error) if attempt >= self.options.max_retries => return Err(error),
                Err(_) => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(self.options.retry_delay_ms)).await;
                }
            }
        }
    }
}

impl SearchBackend for WebSearchAggregator {
    fn id(&self) -> &str {
        "aggregate.web_search"
    }

    fn kind(&self) -> &str {
        "aggregator"
    }
}

#[async_trait]
impl WebSearchBackend for WebSearchAggregator {
    fn endpoints(&self) -> Vec<String> {
        let mut endpoints: Vec<String> = self
            .registry
            .snapshot()
            .into_iter()
            .flat_map(|(_, backend, _)| backend.endpoints())
            .collect();
        endpoints.sort();
        endpoints.dedup();
        endpoints
    }

    fn catalog(&self) -> Vec<SearchBackendInfo> {
        self.registry
            .snapshot()
            .into_iter()
            .map(|(id, backend, _)| SearchBackendInfo {
                id,
                kind: backend.kind().to_string(),
                capabilities: vec!["web_search".to_string()],
            })
            .collect()
    }

    async fn search(&self, query: WebSearchQuery) -> Result<WebSearchResponse, WebSearchError> {
        let id = self.id().to_string();
        let key = cache_key(&query);
        if let Some(response) = self.cached(&key) {
            return Ok(response);
        }
        let work = self.search_inner(query);
        let result =
            match tokio::time::timeout(Duration::from_millis(self.timeout_ms.max(1)), work).await {
                Ok(result) => result,
                Err(_) => Err(WebSearchError::Backend(id, "aggregation timeout".into())),
            };
        if let Ok(response) = &result {
            self.store_cache(key, response);
        }
        result
    }
}

impl WebSearchAggregator {
    fn cached(&self, key: &str) -> Option<WebSearchResponse> {
        if self.options.cache_ttl_ms == 0 {
            return None;
        }
        let ttl = Duration::from_millis(self.options.cache_ttl_ms);
        self.cache
            .get(key)
            .and_then(|entry| (entry.value().0.elapsed() < ttl).then(|| entry.value().1.clone()))
    }

    fn store_cache(&self, key: String, response: &WebSearchResponse) {
        if self.options.cache_ttl_ms > 0 {
            self.cache.insert(key, (Instant::now(), response.clone()));
        }
    }
}

fn cache_key(query: &WebSearchQuery) -> String {
    serde_json::to_string(&(
        &query.query,
        query.max_results,
        &query.backend,
        &query.language,
        &query.region,
        &query.freshness,
        query.page,
    ))
    .unwrap_or_else(|_| query.query.clone())
}

impl WebSearchAggregator {
    async fn search_inner(
        &self,
        query: WebSearchQuery,
    ) -> Result<WebSearchResponse, WebSearchError> {
        let selected = self.selected(query.backend.as_deref())?;
        let outcomes = if self.mode == AggregationMode::All {
            let calls = selected
                .iter()
                .map(|(id, backend)| {
                    let call_query = query.clone();
                    async move { (id.clone(), self.call(&id, backend, &call_query).await) }
                })
                .collect::<Vec<_>>();
            join_all(calls).await
        } else {
            let mut outcomes = Vec::with_capacity(selected.len());
            for (id, backend) in selected {
                let result = self.call(&id, &backend, &query).await;
                let succeeded = matches!(&result, Ok(response) if !response.results.is_empty());
                outcomes.push((id, result));
                if succeeded {
                    break;
                }
            }
            outcomes
        };
        let used: Vec<String> = outcomes.iter().map(|(id, _)| id.clone()).collect();
        let mut errors = Vec::new();
        let mut hits = Vec::new();
        let mut fallback: Option<WebSearchResponse> = None;

        for (id, result) in outcomes {
            match result {
                Ok(response) if !response.results.is_empty() => {
                    if self.mode == AggregationMode::PriorityFirstSuccess {
                        return Ok(with_metadata(response, &used, &errors));
                    }
                    hits.extend(response.results);
                }
                Ok(response) => {
                    if fallback.is_none() {
                        fallback = Some(response);
                    }
                }
                Err(error) => errors.push(json!({
                    "backend": id,
                    "error": error.to_string(),
                })),
            }
        }

        if hits.is_empty() {
            if let Some(response) = fallback {
                return Ok(with_metadata(response, &used, &errors));
            }
            if errors.is_empty() {
                return Err(WebSearchError::NoResults);
            }
            return Err(WebSearchError::Backend(
                "aggregate.web_search".into(),
                "all search backends failed".into(),
            ));
        }
        let limited = dedup(hits, query.max_results.min(max_results_cap()));
        Ok(WebSearchResponse::with_metadata(
            limited,
            json!({ "backends": used, "backend_errors": errors }),
        ))
    }
}

fn with_metadata(
    response: WebSearchResponse,
    used: &[String],
    errors: &[serde_json::Value],
) -> WebSearchResponse {
    WebSearchResponse::with_metadata(
        response.results,
        json!({ "backends": used, "backend_errors": errors }),
    )
}

fn dedup(results: Vec<WebSearchResult>, limit: usize) -> Vec<WebSearchResult> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(limit);
    for result in results {
        if seen.insert(result.url.clone()) {
            out.push(result);
            if out.len() == limit {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubBackend {
        id: String,
        results: Vec<WebSearchResult>,
        fails: bool,
        delay_ms: u64,
    }

    impl StubBackend {
        fn hit(id: &str) -> Self {
            Self::with_url(id, &format!("https://example.com/{id}"))
        }

        fn with_url(id: &str, url: &str) -> Self {
            Self {
                id: id.into(),
                results: vec![WebSearchResult {
                    title: id.into(),
                    url: url.into(),
                    snippet: String::new(),
                }],
                fails: false,
                delay_ms: 0,
            }
        }

        fn failing(id: &str) -> Self {
            Self {
                id: id.into(),
                results: Vec::new(),
                fails: true,
                delay_ms: 0,
            }
        }

        fn delayed(id: &str, delay_ms: u64) -> Self {
            Self {
                delay_ms,
                ..Self::hit(id)
            }
        }
    }

    impl SearchBackend for StubBackend {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &str {
            "stub"
        }
    }

    #[async_trait]
    impl WebSearchBackend for StubBackend {
        async fn search(
            &self,
            _query: WebSearchQuery,
        ) -> Result<WebSearchResponse, WebSearchError> {
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            if self.fails {
                Err(WebSearchError::Backend(
                    self.id.clone(),
                    "stub failure".into(),
                ))
            } else {
                Ok(WebSearchResponse::new(self.results.clone()))
            }
        }
    }

    struct BarrierBackend {
        id: String,
        barrier: Arc<tokio::sync::Barrier>,
    }

    impl SearchBackend for BarrierBackend {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &str {
            "barrier"
        }
    }

    #[async_trait]
    impl WebSearchBackend for BarrierBackend {
        async fn search(
            &self,
            _query: WebSearchQuery,
        ) -> Result<WebSearchResponse, WebSearchError> {
            self.barrier.wait().await;
            Ok(WebSearchResponse::new(vec![WebSearchResult {
                title: self.id.clone(),
                url: format!("https://example.com/{}", self.id),
                snippet: String::new(),
            }]))
        }
    }

    fn query() -> WebSearchQuery {
        WebSearchQuery {
            query: "rust".into(),
            max_results: 5,
            backend: None,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn aggregates_and_reports_partial_failures() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(StubBackend::hit("a")))
            .expect("register a");
        registry
            .register_plugin("p1", Arc::new(StubBackend::failing("b")))
            .expect("register b");
        let aggregator = WebSearchAggregator::new(registry, AggregationMode::All, 1000, None);
        let response = aggregator.search(query()).await.expect("partial success");
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.metadata["backend_errors"][0]["backend"], "b");
    }

    #[tokio::test]
    async fn explicit_backend_routes_to_one_backend() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(StubBackend::hit("a")))
            .expect("register a");
        registry
            .register_builtin(Arc::new(StubBackend::hit("b")))
            .expect("register b");
        let aggregator = WebSearchAggregator::new(registry, AggregationMode::All, 1000, None);
        let mut call = query();
        call.backend = Some("b".into());
        let response = aggregator.search(call).await.expect("explicit backend");
        assert_eq!(response.results[0].title, "b");
    }

    #[tokio::test]
    async fn all_mode_calls_backends_concurrently() {
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(BarrierBackend {
                id: "a".into(),
                barrier: barrier.clone(),
            }))
            .expect("register a");
        registry
            .register_builtin(Arc::new(BarrierBackend {
                id: "b".into(),
                barrier,
            }))
            .expect("register b");
        let aggregator = WebSearchAggregator::new(registry, AggregationMode::All, 1000, None);
        let response = aggregator
            .search(query())
            .await
            .expect("both backends arrive");
        assert_eq!(response.results.len(), 2);
    }

    #[tokio::test]
    async fn dedups_results_by_url() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(StubBackend::with_url(
                "a",
                "https://example.com/shared",
            )))
            .expect("register a");
        registry
            .register_builtin(Arc::new(StubBackend::with_url(
                "b",
                "https://example.com/shared",
            )))
            .expect("register b");
        let aggregator = WebSearchAggregator::new(registry, AggregationMode::All, 1000, None);
        let response = aggregator.search(query()).await.expect("dedup succeeds");
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].title, "a");
        assert_eq!(response.metadata["backends"], json!(["a", "b"]));
    }

    #[tokio::test]
    async fn priority_mode_stops_at_first_non_empty_result() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(StubBackend::failing("a")))
            .expect("register a");
        registry
            .register_builtin(Arc::new(StubBackend::hit("b")))
            .expect("register b");
        let aggregator =
            WebSearchAggregator::new(registry, AggregationMode::PriorityFirstSuccess, 1000, None);
        let response = aggregator.search(query()).await.expect("priority success");
        assert_eq!(response.results[0].title, "b");
        assert_eq!(response.metadata["backends"], json!(["a", "b"]));
        assert_eq!(response.metadata["backend_errors"][0]["backend"], "a");
    }

    #[tokio::test]
    async fn default_backends_limits_selection() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(StubBackend::hit("a")))
            .expect("register a");
        registry
            .register_builtin(Arc::new(StubBackend::hit("b")))
            .expect("register b");
        let aggregator =
            WebSearchAggregator::new(registry, AggregationMode::All, 1000, Some(vec!["b".into()]));
        let response = aggregator.search(query()).await.expect("subset succeeds");
        assert_eq!(response.results[0].title, "b");
        assert_eq!(response.metadata["backends"], json!(["b"]));
    }

    #[tokio::test]
    async fn explicit_backend_respects_configured_allowlist() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(StubBackend::hit("a")))
            .expect("register a");
        registry
            .register_builtin(Arc::new(StubBackend::hit("b")))
            .expect("register b");
        let aggregator =
            WebSearchAggregator::new(registry, AggregationMode::All, 1000, Some(vec!["b".into()]));
        let mut call = query();
        call.backend = Some("a".into());
        assert!(aggregator.search(call).await.is_err());
    }

    #[tokio::test]
    async fn cache_avoids_duplicate_backend_calls() {
        struct CountingBackend(Arc<AtomicUsize>);

        impl SearchBackend for CountingBackend {
            fn id(&self) -> &str {
                "counting"
            }
            fn kind(&self) -> &str {
                "test"
            }
        }

        #[async_trait]
        impl WebSearchBackend for CountingBackend {
            async fn search(
                &self,
                _query: WebSearchQuery,
            ) -> Result<WebSearchResponse, WebSearchError> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(WebSearchResponse::new(vec![WebSearchResult {
                    title: "cached".into(),
                    url: "https://example.com/cached".into(),
                    snippet: String::new(),
                }]))
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(CountingBackend(calls.clone())))
            .expect("register counting backend");
        let aggregator = WebSearchAggregator::new_with_options(
            registry,
            AggregationMode::All,
            1000,
            None,
            SearchAggregationOptions {
                cache_ttl_ms: 10_000,
                ..Default::default()
            },
        );
        aggregator.search(query()).await.expect("first search");
        aggregator.search(query()).await.expect("second search");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn aggregation_timeout_aborts_slow_backends() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(StubBackend::delayed("slow", 200)))
            .expect("register slow");
        let aggregator = WebSearchAggregator::new(registry, AggregationMode::All, 20, None);
        let error = aggregator
            .search(query())
            .await
            .expect_err("slow backend exceeds aggregation timeout");
        assert!(error.to_string().contains("aggregation timeout"));
    }

    #[test]
    fn plugin_backends_are_removed_by_owner() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(StubBackend::hit("a")))
            .expect("register a");
        registry
            .register_plugin("p1", Arc::new(StubBackend::hit("b")))
            .expect("register b");
        assert_eq!(registry.unregister_by_owner("p1"), vec!["b".to_string()]);
        assert_eq!(registry.ids(), vec!["a".to_string()]);
    }
}

//! Search backend registry and web-domain aggregation.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::json;
use wf_config::processor::network::{max_results_cap, AggregationMode};
use wf_tools::{
    SearchBackend, WebSearchBackend, WebSearchError, WebSearchQuery, WebSearchResponse,
    WebSearchResult,
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

/// Aggregates the registered web backends behind one tool backend.
pub struct WebSearchAggregator {
    registry: NetworkRegistry,
    mode: AggregationMode,
    timeout_ms: u64,
    default_backends: Option<Vec<String>>,
}

impl WebSearchAggregator {
    pub fn new(
        registry: NetworkRegistry,
        mode: AggregationMode,
        timeout_ms: u64,
        default_backends: Option<Vec<String>>,
    ) -> Self {
        Self {
            registry,
            mode,
            timeout_ms,
            default_backends,
        }
    }

    fn selected(&self, requested: Option<&str>) -> Result<Vec<SelectedBackend>, WebSearchError> {
        let entries = self.registry.snapshot();
        if let Some(id) = requested {
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

    async fn call(
        &self,
        backend: &Arc<dyn WebSearchBackend>,
        query: &WebSearchQuery,
    ) -> Result<WebSearchResponse, WebSearchError> {
        backend
            .search(WebSearchQuery {
                query: query.query.clone(),
                max_results: query.max_results,
                backend: None,
            })
            .await
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
    async fn search(&self, query: WebSearchQuery) -> Result<WebSearchResponse, WebSearchError> {
        let id = self.id().to_string();
        let work = self.search_inner(query);
        match tokio::time::timeout(Duration::from_millis(self.timeout_ms.max(1)), work).await {
            Ok(result) => result,
            Err(_) => Err(WebSearchError::Backend(id, "aggregation timeout".into())),
        }
    }
}

impl WebSearchAggregator {
    async fn search_inner(
        &self,
        query: WebSearchQuery,
    ) -> Result<WebSearchResponse, WebSearchError> {
        let selected = self.selected(query.backend.as_deref())?;
        let mut errors = Vec::new();
        let mut hits = Vec::new();
        let mut used = Vec::new();
        let mut fallback: Option<WebSearchResponse> = None;

        for (id, backend) in selected {
            used.push(id.clone());
            match self.call(&backend, &query).await {
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
    }

    impl StubBackend {
        fn hit(id: &str) -> Self {
            Self {
                id: id.into(),
                results: vec![WebSearchResult {
                    title: id.into(),
                    url: format!("https://example.com/{id}"),
                    snippet: String::new(),
                }],
                fails: false,
            }
        }

        fn failing(id: &str) -> Self {
            Self {
                id: id.into(),
                results: Vec::new(),
                fails: true,
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

    fn query() -> WebSearchQuery {
        WebSearchQuery {
            query: "rust".into(),
            max_results: 5,
            backend: None,
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

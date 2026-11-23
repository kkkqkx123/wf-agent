//! Built-in HTTP fetch backend, registry and router.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use dashmap::DashMap;
use futures::StreamExt;
use wf_tools::{
    redact_transport_error, FetchToolConfig, WebFetchBackend, WebFetchError, WebFetchQuery,
    WebFetchResponse,
};

use super::backend::SearchBackendClient;

const BUILTIN_FETCH_ID: &str = "http";

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
}

impl WebFetchRouter {
    pub fn new(registry: FetchBackendRegistry, selected: impl Into<String>) -> Self {
        let router = Self { registry };
        // Selection is owned by the registry; ignore a missing id here and let
        // fetch report it, so construction stays infallible.
        let _ = router.registry.set_selected(&selected.into());
        router
    }
}

#[async_trait]
impl WebFetchBackend for WebFetchRouter {
    async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
        let selected = self.registry.selected();
        let backend = self.registry.get(&selected).ok_or_else(|| {
            WebFetchError::Transport(format!(
                "fetch backend '{}' is not registered",
                selected
            ))
        })?;
        backend.fetch(query).await
    }

    fn id(&self) -> &str {
        "route.web_fetch"
    }

    fn kind(&self) -> &str {
        "router"
    }
}

/// Default HTTP implementation with streaming size and redirect guards.
pub struct HttpFetchBackend {
    id: String,
    client: reqwest::Client,
    max_content_bytes: usize,
}

impl HttpFetchBackend {
    pub fn new(id: impl Into<String>, config: &FetchToolConfig) -> Result<Self, String> {
        let client = SearchBackendClient::build(
            config.timeout_ms,
            &config.user_agent,
            &config.proxy,
            &config.domain_policy,
            config.max_redirects,
        )?;
        Ok(Self {
            id: id.into(),
            client,
            max_content_bytes: config.max_content_bytes,
        })
    }
}

#[async_trait]
impl WebFetchBackend for HttpFetchBackend {
    async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
        let parsed = reqwest::Url::parse(&query.url).map_err(|_| WebFetchError::InvalidUrl)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(WebFetchError::UnsupportedScheme);
        }
        let response = self
            .client
            .get(parsed)
            .send()
            .await
            .map_err(|error| WebFetchError::Transport(redact_transport_error(error)))?;
        let final_url = response.url().to_string();
        let status = response.status();
        if !status.is_success() {
            return Err(WebFetchError::Transport(format!(
                "request to '{final_url}' failed with status {status}"
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();

        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|error| WebFetchError::Transport(redact_transport_error(error)))?;
            if bytes.len() + chunk.len() > self.max_content_bytes {
                return Err(WebFetchError::TooLarge {
                    size: bytes.len() + chunk.len(),
                    limit: self.max_content_bytes,
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(WebFetchResponse {
            final_url,
            content_type,
            content: String::from_utf8_lossy(&bytes).into_owned(),
            size: bytes.len(),
        })
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &str {
        "http"
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
                content_type: "text/plain".into(),
                content: self.tag.clone(),
                size: self.tag.len(),
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

        let router = WebFetchRouter::new(registry.clone(), "http");
        assert_eq!(registry.selected(), "http");
        assert!(registry.set_selected("p1.fetch").is_ok());
        assert_eq!(registry.selected(), "p1.fetch");
        assert!(registry.set_selected("missing").is_err());
        assert_eq!(registry.selected(), "p1.fetch");

        let query = WebFetchQuery {
            url: "https://example.com".into(),
        };
        let routed = futures::executor::block_on(router.fetch(query)).expect("fetch succeeds");
        assert_eq!(routed.content, "plugin");
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
}

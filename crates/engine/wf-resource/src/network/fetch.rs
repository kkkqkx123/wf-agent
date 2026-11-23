//! Built-in HTTP fetch backend, registry and router.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use futures::StreamExt;
use wf_tools::{
    redact_transport_error, FetchToolConfig, WebFetchBackend, WebFetchError, WebFetchQuery,
    WebFetchResponse,
};

use super::backend::SearchBackendClient;

struct FetchEntry {
    backend: Arc<dyn WebFetchBackend>,
    owner: String,
}

/// Shared registry of fetch backends with owner-based teardown.
#[derive(Clone, Default)]
pub struct FetchBackendRegistry {
    entries: Arc<DashMap<String, FetchEntry>>,
}

impl FetchBackendRegistry {
    pub fn new() -> Self {
        Self::default()
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

/// Routes every fetch call to the configured backend id.
pub struct WebFetchRouter {
    registry: FetchBackendRegistry,
    selected: String,
}

impl WebFetchRouter {
    pub fn new(registry: FetchBackendRegistry, selected: impl Into<String>) -> Self {
        Self {
            registry,
            selected: selected.into(),
        }
    }
}

#[async_trait]
impl WebFetchBackend for WebFetchRouter {
    async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
        let backend = self.registry.get(&self.selected).ok_or_else(|| {
            WebFetchError::Transport(format!(
                "fetch backend '{}' is not registered",
                self.selected
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

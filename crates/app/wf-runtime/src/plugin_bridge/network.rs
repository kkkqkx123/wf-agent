//! Plugin-backed web search backends.
//!
//! Plugins contribute a normal `tool` plus a `tool-type` executor. A tool
//! tagged `search-backend:web` is adapted into a backend and joined to the
//! shared aggregation registry for its plugin lifetime.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::warn;
use wf_plugin::{
    ContributionManager, PluginPermission, PluginRegistry, PluginToolContext, PluginToolExecutor,
    PluginType,
};
use wf_resource::NetworkRegistry;
use wf_tools::{SearchBackend, WebSearchError, WebSearchQuery, WebSearchResponse, WebSearchResult};

const WEB_BACKEND_TAG: &str = "search-backend:web";

struct PluginWebSearchBackend {
    id: String,
    executor: Arc<dyn PluginToolExecutor>,
}

impl SearchBackend for PluginWebSearchBackend {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &str {
        "plugin"
    }
}

#[async_trait]
impl wf_tools::WebSearchBackend for PluginWebSearchBackend {
    async fn search(&self, query: WebSearchQuery) -> Result<WebSearchResponse, WebSearchError> {
        let args = json!({
            "query": query.query,
            "max_results": query.max_results,
        });
        let output = self
            .executor
            .execute(PluginToolContext { args })
            .await
            .map_err(|error| WebSearchError::Backend(self.id.clone(), error.to_string()))?
            .result;
        parse_plugin_results(&self.id, &output)
    }
}

fn parse_plugin_results(id: &str, output: &Value) -> Result<WebSearchResponse, WebSearchError> {
    let items = output
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            WebSearchError::Backend(id.to_string(), "plugin result has no results array".into())
        })?;
    let mut results = Vec::with_capacity(items.len());
    for item in items {
        let title = item.get("title").and_then(Value::as_str);
        let url = item.get("url").and_then(Value::as_str);
        if title.is_none() || url.is_none() {
            continue;
        }
        results.push(WebSearchResult {
            title: title.unwrap_or_default().to_string(),
            url: url.unwrap_or_default().to_string(),
            snippet: item
                .get("snippet")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    }
    Ok(WebSearchResponse::with_metadata(
        results,
        json!({ "plugin": id }),
    ))
}

fn is_web_backend(tool_id: &str, manager: &ContributionManager) -> bool {
    manager
        .get_tool(tool_id)
        .and_then(|tool| tool.metadata.clone())
        .and_then(|metadata| metadata.tags)
        .is_some_and(|tags| tags.iter().any(|tag| tag == WEB_BACKEND_TAG))
}

fn plugin_can_search(plugin_id: &str, plugin_registry: &PluginRegistry) -> bool {
    let Some(info) = plugin_registry.get(plugin_id) else {
        return true;
    };
    if info.manifest.plugin_type != Some(PluginType::Native) {
        warn!(
            plugin_id,
            "plugin search backend requires the native backend"
        );
        return false;
    }
    if !info.permissions.contains(&PluginPermission::Network) {
        warn!(
            plugin_id,
            "plugin search backend requires the network permission"
        );
        return false;
    }
    true
}

pub(crate) fn sync_network(
    registry: &NetworkRegistry,
    plugin_id: &str,
    manager: &ContributionManager,
    plugin_registry: &PluginRegistry,
) {
    if !plugin_can_search(plugin_id, plugin_registry) {
        return;
    }
    let executor = manager
        .all_tool_types()
        .into_iter()
        .filter(|(_, owner)| owner == plugin_id)
        .filter_map(|(name, _)| manager.get_tool_executor(&name))
        .last();
    let Some(executor) = executor else {
        return;
    };
    for (id, owner) in manager.all_tools() {
        if owner != plugin_id || !is_web_backend(&id, manager) {
            continue;
        }
        let backend = PluginWebSearchBackend {
            id: id.clone(),
            executor: executor.clone(),
        };
        if let Err(error) = registry.register_plugin(plugin_id, Arc::new(backend)) {
            warn!(plugin_id, backend = %id, error = %error, "plugin search backend was not registered");
        }
    }
}

pub(crate) fn unsync_network(registry: &NetworkRegistry, plugin_id: &str) {
    let removed = registry.unregister_by_owner(plugin_id);
    if !removed.is_empty() {
        tracing::debug!(plugin_id, backends = ?removed, "plugin search backends removed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_plugin_results() {
        let output = json!({ "results": [{ "title": "T", "url": "https://x", "snippet": "S" }] });
        let response = parse_plugin_results("p", &output).expect("valid results");
        assert_eq!(response.results[0].title, "T");
    }

    #[test]
    fn rejects_missing_results_array() {
        assert!(parse_plugin_results("p", &json!({})).is_err());
    }
}

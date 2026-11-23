//! Plugin-backed web search backends.
//!
//! Plugins contribute a normal `tool` plus a `tool-type` executor. A tool
//! tagged `search-backend:web` is adapted into a backend and joined to the
//! shared aggregation registry for its plugin lifetime.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use wf_plugin::{
    ContributionManager, PluginError, PluginPermission, PluginRegistry, PluginResult,
    PluginToolContext, PluginToolExecutor, PluginType,
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

fn validate_plugin_search(plugin_id: &str, plugin_registry: &PluginRegistry) -> PluginResult<()> {
    let Some(info) = plugin_registry.get(plugin_id) else {
        return Ok(());
    };
    if info.manifest.plugin_type != Some(PluginType::Native) {
        return Err(PluginError::InvalidContribution {
            plugin_id: plugin_id.into(),
            message: "search backend requires the native plugin backend".into(),
        });
    }
    if !info.permissions.contains(&PluginPermission::Network) {
        return Err(PluginError::PermissionDenied {
            plugin_id: plugin_id.into(),
            reason: "search backend requires the network permission".into(),
        });
    }
    Ok(())
}

pub(crate) fn sync_network(
    registry: &NetworkRegistry,
    plugin_id: &str,
    manager: &ContributionManager,
    plugin_registry: &PluginRegistry,
) -> PluginResult<()> {
    validate_plugin_search(plugin_id, plugin_registry)?;
    let backends: Vec<String> = manager
        .all_tools()
        .into_iter()
        .filter(|(id, owner)| owner == plugin_id && is_web_backend(id, manager))
        .map(|(id, _)| id)
        .collect();
    if backends.is_empty() {
        return Ok(());
    }
    let executor = manager
        .all_tool_types()
        .into_iter()
        .filter(|(_, owner)| owner == plugin_id)
        .filter_map(|(name, _)| manager.get_tool_executor(&name))
        .last()
        .ok_or_else(|| PluginError::InvalidContribution {
            plugin_id: plugin_id.into(),
            message: format!(
                "search backend tool '{}' has no tool-type executor",
                backends[0]
            ),
        })?;
    for id in backends {
        let backend = PluginWebSearchBackend {
            id: id.clone(),
            executor: executor.clone(),
        };
        registry
            .register_plugin(plugin_id, Arc::new(backend))
            .map_err(|error| {
                PluginError::ContributionConflict(format!(
                    "search backend id '{id}' cannot be registered by plugin '{plugin_id}': {error}"
                ))
            })?;
    }
    Ok(())
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
    use wf_plugin::{ContributionRegistrar, PluginToolResult};
    use wf_tools::WebSearchBackend;
    use wf_types::tool::{Tool, ToolMetadata, ToolType};

    struct SearchExecutor;

    #[async_trait]
    impl PluginToolExecutor for SearchExecutor {
        async fn execute(&self, _ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
            Ok(PluginToolResult {
                result: json!({
                    "results": [{ "title": "T", "url": "https://x", "snippet": "S" }]
                }),
            })
        }
    }

    struct FailingExecutor;

    #[async_trait]
    impl PluginToolExecutor for FailingExecutor {
        async fn execute(&self, _ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
            Err(PluginError::Internal("executor failed".into()))
        }
    }

    struct ConflictBackend;

    impl SearchBackend for ConflictBackend {
        fn id(&self) -> &str {
            "p1.search"
        }
        fn kind(&self) -> &str {
            "test"
        }
    }

    #[async_trait]
    impl WebSearchBackend for ConflictBackend {
        async fn search(
            &self,
            _query: WebSearchQuery,
        ) -> Result<WebSearchResponse, WebSearchError> {
            Ok(WebSearchResponse::default())
        }
    }

    fn tagged_tool(id: &str) -> Tool {
        Tool {
            id: id.into(),
            name: format!("{id} tool"),
            description: "plugin search backend".into(),
            tool_type: ToolType::Stateless,
            parameters: None,
            metadata: Some(ToolMetadata {
                category: None,
                tags: Some(vec![WEB_BACKEND_TAG.into()]),
                documentation_url: None,
                custom_fields: None,
                risk_level: None,
                auto_approvable: None,
                create_checkpoint: None,
                exposure: None,
            }),
            config: None,
            enabled: None,
            strict: None,
            default_timeout_ms: None,
        }
    }

    fn manager_with_search(plugin_id: &str, tool_id: &str) -> ContributionManager {
        let manager = ContributionManager::new();
        manager.start_registration(plugin_id);
        manager
            .as_registrar()
            .register_tool_type("p1.search_type", Arc::new(SearchExecutor))
            .expect("register executor");
        manager
            .as_registrar()
            .register_tool(tool_id, tagged_tool(tool_id))
            .expect("register tagged tool");
        manager
    }

    fn query() -> WebSearchQuery {
        WebSearchQuery {
            query: "rust".into(),
            max_results: 5,
            backend: None,
        }
    }

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

    #[test]
    fn sync_registers_and_unsync_removes_plugin_backend() {
        let registry = NetworkRegistry::new();
        let manager = manager_with_search("p1", "p1.search");
        let plugins = PluginRegistry::new();

        sync_network(&registry, "p1", &manager, &plugins).expect("sync succeeds");
        assert!(registry.has("p1.search"));

        unsync_network(&registry, "p1");
        assert!(!registry.has("p1.search"));
    }

    #[test]
    fn duplicate_backend_id_fails_plugin_sync() {
        let registry = NetworkRegistry::new();
        registry
            .register_builtin(Arc::new(ConflictBackend))
            .expect("register existing backend");
        let manager = manager_with_search("p1", "p1.search");
        let plugins = PluginRegistry::new();

        assert!(sync_network(&registry, "p1", &manager, &plugins).is_err());
    }

    #[tokio::test]
    async fn executor_failure_is_rejected() {
        let backend = PluginWebSearchBackend {
            id: "failing".into(),
            executor: Arc::new(FailingExecutor),
        };
        let error = backend
            .search(query())
            .await
            .expect_err("executor failure propagates");
        assert!(error.to_string().contains("executor failed"));
    }
}

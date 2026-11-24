//! Plugin-backed web search and fetch backends.
//!
//! Plugins contribute a normal `tool` plus a `tool-type` executor. Tags select
//! the capability a tool implements; matching backends join the shared
//! registries for the plugin lifetime.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use wf_plugin::{
    ContributionManager, PluginError, PluginPermission, PluginRegistry, PluginResult,
    PluginToolContext, PluginToolExecutor, PluginType,
};
use wf_resource::{FetchBackendRegistry, NetworkRegistry};
use wf_tools::{
    FetchBackendInfo, SearchBackend, WebFetchBackend, WebFetchError, WebFetchQuery,
    WebFetchResponse, WebSearchError, WebSearchQuery, WebSearchResponse, WebSearchResult,
};

const SEARCH_BACKEND_TAG: &str = "search-backend:web";
const FETCH_BACKEND_TAG: &str = "fetch-backend:web";

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

struct PluginWebFetchBackend {
    id: String,
    executor: Arc<dyn PluginToolExecutor>,
}

#[async_trait]
impl WebFetchBackend for PluginWebFetchBackend {
    async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
        let link_mode = format!("{:?}", query.link_mode).to_ascii_lowercase();
        let args = json!({
            "url": query.url.clone(),
            "render": query.render,
            "wait_for_ms": query.wait_for_ms,
            "selector": query.selector,
            "exclude_selector": query.exclude_selector,
            "focus": query.focus,
            "min_words": query.min_words,
            "link_mode": link_mode,
            "refresh": query.refresh,
            "max_chars": query.max_chars,
        });
        let output = self
            .executor
            .execute(PluginToolContext { args })
            .await
            .map_err(|error| WebFetchError::Transport(error.to_string()))?
            .result;
        let claims = |key: &str| match output.get("options_applied") {
            Some(Value::Bool(value)) => *value,
            Some(Value::Object(values)) => {
                values.get(key).and_then(Value::as_bool).unwrap_or(false)
            }
            _ => false,
        };
        for (key, requested) in [
            ("render", query.render),
            ("wait_for_ms", query.wait_for_ms.is_some()),
            ("selector", query.selector.is_some()),
            ("exclude_selector", query.exclude_selector.is_some()),
            ("focus", query.focus.is_some()),
            ("min_words", query.min_words > 0),
            ("refresh", query.refresh),
        ] {
            if requested && !claims(key) {
                return Err(WebFetchError::UnsupportedOption(key));
            }
        }
        let content_options = [
            ("selector", query.selector.is_some()),
            ("exclude_selector", query.exclude_selector.is_some()),
            ("focus", query.focus.is_some()),
            ("min_words", query.min_words > 0),
            ("refresh", query.refresh),
        ];
        let options_applied = claims("all")
            || content_options
                .iter()
                .all(|(key, requested)| !requested || claims(key));
        let content = output
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| WebFetchError::Transport("plugin result has no content".into()))?
            .to_string();
        let final_url = output
            .get("final_url")
            .and_then(Value::as_str)
            .unwrap_or(&query.url)
            .to_string();
        let status_code = output
            .get("status_code")
            .and_then(Value::as_u64)
            .unwrap_or(200);
        let headers = output
            .get("response_headers")
            .or_else(|| output.get("headers"))
            .and_then(Value::as_object)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|(name, value)| {
                        value
                            .as_str()
                            .map(|value| (name.clone(), value.to_string()))
                    })
                    .collect::<std::collections::BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        Ok(WebFetchResponse {
            final_url,
            status_code: u16::try_from(status_code).map_err(|_| {
                WebFetchError::Transport("plugin status_code is out of range".into())
            })?,
            headers,
            content_type: output
                .get("content_type")
                .and_then(Value::as_str)
                .unwrap_or("text/plain")
                .to_string(),
            size: output
                .get("size")
                .and_then(Value::as_u64)
                .unwrap_or(content.len() as u64) as usize,
            cache_status: output
                .get("cache_status")
                .and_then(Value::as_str)
                .unwrap_or("unavailable")
                .to_string(),
            elapsed_ms: output
                .get("elapsed_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            title: output
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
            metadata: output.get("metadata").cloned(),
            links: output.get("links").cloned(),
            tables: output.get("tables").cloned(),
            content,
            options_applied,
        })
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &str {
        "plugin"
    }

    fn catalog(&self) -> Vec<FetchBackendInfo> {
        vec![FetchBackendInfo {
            id: self.id.clone(),
            kind: "plugin".to_string(),
            capabilities: vec![
                "web_fetch".to_string(),
                "render".to_string(),
                "wait_for".to_string(),
                "selector".to_string(),
                "exclude_selector".to_string(),
                "focus".to_string(),
                "min_words".to_string(),
                "link_mode".to_string(),
                "clean_html".to_string(),
            ],
        }]
    }
}

fn has_tag(tool_id: &str, tag: &str, manager: &ContributionManager) -> bool {
    manager
        .get_tool(tool_id)
        .and_then(|tool| tool.metadata.clone())
        .and_then(|metadata| metadata.tags)
        .is_some_and(|tags| tags.iter().any(|candidate| candidate == tag))
}

fn validate_plugin_network(plugin_id: &str, plugin_registry: &PluginRegistry) -> PluginResult<()> {
    let Some(info) = plugin_registry.get(plugin_id) else {
        return Ok(());
    };
    if info.manifest.plugin_type != Some(PluginType::Native) {
        return Err(PluginError::InvalidContribution {
            plugin_id: plugin_id.into(),
            message: "network backend requires the native plugin backend".into(),
        });
    }
    if !info.permissions.contains(&PluginPermission::Network) {
        return Err(PluginError::PermissionDenied {
            plugin_id: plugin_id.into(),
            reason: "network backend requires the network permission".into(),
        });
    }
    Ok(())
}

pub(crate) fn sync_network(
    registry: &NetworkRegistry,
    fetch_registry: &FetchBackendRegistry,
    plugin_id: &str,
    manager: &ContributionManager,
    plugin_registry: &PluginRegistry,
) -> PluginResult<()> {
    let search_backends: Vec<String> = manager
        .all_tools()
        .into_iter()
        .filter(|(id, owner)| owner == plugin_id && has_tag(id, SEARCH_BACKEND_TAG, manager))
        .map(|(id, _)| id)
        .collect();
    let fetch_backends: Vec<String> = manager
        .all_tools()
        .into_iter()
        .filter(|(id, owner)| owner == plugin_id && has_tag(id, FETCH_BACKEND_TAG, manager))
        .map(|(id, _)| id)
        .collect();
    if search_backends.is_empty() && fetch_backends.is_empty() {
        return Ok(());
    }
    validate_plugin_network(plugin_id, plugin_registry)?;
    let executors: Vec<_> = manager
        .all_tool_types()
        .into_iter()
        .filter(|(_, owner)| owner == plugin_id)
        .filter_map(|(name, _)| manager.get_tool_executor(&name))
        .collect();
    if executors.len() != 1 {
        return Err(PluginError::InvalidContribution {
            plugin_id: plugin_id.into(),
            message: format!(
                "network backend requires exactly one tool-type executor, found {}",
                executors.len()
            ),
        });
    }
    let executor = executors
        .into_iter()
        .next()
        .expect("invariant: executor count was checked");
    for id in search_backends {
        registry
            .register_plugin(
                plugin_id,
                Arc::new(PluginWebSearchBackend {
                    id: id.clone(),
                    executor: executor.clone(),
                }),
            )
            .map_err(|error| {
                PluginError::ContributionConflict(format!(
                "search backend id '{id}' cannot be registered by plugin '{plugin_id}': {error}"
            ))
            })?;
    }
    for id in fetch_backends {
        fetch_registry
            .register_plugin(
                plugin_id,
                Arc::new(PluginWebFetchBackend {
                    id: id.clone(),
                    executor: executor.clone(),
                }),
            )
            .map_err(|error| {
                PluginError::ContributionConflict(format!(
                    "fetch backend id '{id}' cannot be registered by plugin '{plugin_id}': {error}"
                ))
            })?;
    }
    Ok(())
}

pub(crate) fn unsync_network(
    registry: &NetworkRegistry,
    fetch_registry: &FetchBackendRegistry,
    plugin_id: &str,
) {
    let removed = registry.unregister_by_owner(plugin_id);
    let fetch_removed = fetch_registry.unregister_by_owner(plugin_id);
    if !removed.is_empty() || !fetch_removed.is_empty() {
        tracing::debug!(plugin_id, search = ?removed, fetch = ?fetch_removed, "plugin network backends removed");
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
                tags: Some(vec![SEARCH_BACKEND_TAG.into()]),
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
            ..Default::default()
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
        let fetch_registry = FetchBackendRegistry::new();
        let manager = manager_with_search("p1", "p1.search");
        let plugins = PluginRegistry::new();

        sync_network(&registry, &fetch_registry, "p1", &manager, &plugins).expect("sync succeeds");
        assert!(registry.has("p1.search"));

        unsync_network(&registry, &fetch_registry, "p1");
        assert!(!registry.has("p1.search"));
    }

    #[test]
    fn duplicate_backend_id_fails_plugin_sync() {
        let registry = NetworkRegistry::new();
        let fetch_registry = FetchBackendRegistry::new();
        registry
            .register_builtin(Arc::new(ConflictBackend))
            .expect("register existing backend");
        let manager = manager_with_search("p1", "p1.search");
        let plugins = PluginRegistry::new();

        assert!(sync_network(&registry, &fetch_registry, "p1", &manager, &plugins).is_err());
    }

    #[test]
    fn multiple_executors_are_rejected() {
        let registry = NetworkRegistry::new();
        let fetch_registry = FetchBackendRegistry::new();
        let manager = manager_with_search("p1", "p1.search");
        manager
            .as_registrar()
            .register_tool_type("p1.other_type", Arc::new(SearchExecutor))
            .expect("register second executor");
        let plugins = PluginRegistry::new();
        let error = sync_network(&registry, &fetch_registry, "p1", &manager, &plugins)
            .expect_err("ambiguous executor must fail");
        assert!(error
            .to_string()
            .contains("requires exactly one tool-type executor"));
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

    struct FetchExecutor;

    #[async_trait]
    impl PluginToolExecutor for FetchExecutor {
        async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
            assert_eq!(ctx.args["render"], json!(true));
            assert_eq!(ctx.args["wait_for_ms"], json!(100));
            assert_eq!(ctx.args["selector"], json!("main"));
            assert_eq!(ctx.args["max_chars"], json!(500));
            Ok(PluginToolResult {
                result: json!({
                    "content": "<main>Ready</main>",
                    "status_code": 201,
                    "headers": { "etag": "v1" },
                    "content_type": "text/html",
                    "size": 18,
                    "title": "Ready",
                    "options_applied": {
                        "render": true,
                        "wait_for_ms": true,
                        "selector": true
                    }
                }),
            })
        }
    }

    #[tokio::test]
    async fn fetch_backend_forwards_options_and_reads_metadata() {
        let backend = PluginWebFetchBackend {
            id: "p1.fetch".into(),
            executor: Arc::new(FetchExecutor),
        };
        let mut fetch_query = WebFetchQuery::new("https://example.com/page");
        fetch_query.render = true;
        fetch_query.wait_for_ms = Some(100);
        fetch_query.selector = Some("main".into());
        fetch_query.max_chars = Some(500);
        let response = backend
            .fetch(fetch_query)
            .await
            .expect("plugin fetch succeeds");
        assert_eq!(response.status_code, 201);
        assert_eq!(response.headers["etag"], "v1");
        assert_eq!(response.title.as_deref(), Some("Ready"));
        assert!(response.options_applied);
    }

    #[tokio::test]
    async fn rejects_requested_option_without_plugin_claim() {
        struct SilentExecutor;

        #[async_trait]
        impl PluginToolExecutor for SilentExecutor {
            async fn execute(&self, _ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
                Ok(PluginToolResult {
                    result: json!({ "content": "<main>Ready</main>" }),
                })
            }
        }

        let backend = PluginWebFetchBackend {
            id: "p1.fetch".into(),
            executor: Arc::new(SilentExecutor),
        };
        let mut fetch_query = WebFetchQuery::new("https://example.com/page");
        fetch_query.render = true;
        let error = backend
            .fetch(fetch_query)
            .await
            .expect_err("unclaimed render option fails");
        assert!(error.to_string().contains("render"));
    }
}

//! MCP tool metadata caching and LLM-visible dynamic context generation.
//!
//! - [`McpToolMetadataCache`] caches each server's discovered tools /
//!   resources behind a TTL, avoiding repeated `tools/list` round trips.
//! - [`McpToolsDynamicContextProvider`] renders a compact, customizable
//!   summary of the configured MCP servers for injection into LLM prompts.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::mcp::analytics::McpUsageAnalytics;
use crate::mcp::client::McpToolInfo;
use crate::mcp::connection::{McpConnectionManager, McpServerEntry};

/// Default TTL for cached metadata (5 minutes).
pub const DEFAULT_METADATA_TTL: Duration = Duration::from_secs(300);

struct CacheEntry {
    tools: Vec<McpToolInfo>,
    resources: Vec<wf_types::tool::McpResource>,
    resource_templates: Vec<wf_types::tool::McpResourceTemplate>,
    instructions: Option<String>,
    timestamp: Instant,
    ttl: Duration,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.timestamp.elapsed() >= self.ttl
    }
}

/// TTL cache of per-server MCP metadata.
pub struct McpToolMetadataCache {
    cache: Mutex<HashMap<String, CacheEntry>>,
    default_ttl: Duration,
    max_size: usize,
}

impl Default for McpToolMetadataCache {
    fn default() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            default_ttl: DEFAULT_METADATA_TTL,
            max_size: 1_000,
        }
    }
}

impl McpToolMetadataCache {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            default_ttl,
            max_size: 1_000,
        }
    }

    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    pub fn set(
        &self,
        server_name: &str,
        tools: Vec<McpToolInfo>,
        resources: Vec<wf_types::tool::McpResource>,
        resource_templates: Vec<wf_types::tool::McpResourceTemplate>,
        instructions: Option<String>,
        ttl: Option<Duration>,
    ) {
        let mut cache = wf_common::lock::lock_ok(self.cache.lock());
        if !cache.contains_key(server_name) && cache.len() >= self.max_size {
            return;
        }
        cache.insert(
            server_name.to_string(),
            CacheEntry {
                tools,
                resources,
                resource_templates,
                instructions,
                timestamp: Instant::now(),
                ttl: ttl.unwrap_or(self.default_ttl),
            },
        );
    }

    pub fn get(&self, server_name: &str) -> Option<CachedServerMetadata> {
        let mut cache = wf_common::lock::lock_ok(self.cache.lock());
        let entry = cache.get(server_name)?;
        if entry.is_expired() {
            cache.remove(server_name);
            return None;
        }
        Some(CachedServerMetadata {
            tools: entry.tools.clone(),
            resources: entry.resources.clone(),
            resource_templates: entry.resource_templates.clone(),
            instructions: entry.instructions.clone(),
        })
    }

    pub fn has(&self, server_name: &str) -> bool {
        self.get(server_name).is_some()
    }

    pub fn keys(&self) -> Vec<String> {
        let mut cache = wf_common::lock::lock_ok(self.cache.lock());
        cache.retain(|_, entry| !entry.is_expired());
        cache.keys().cloned().collect()
    }

    pub fn invalidate(&self, server_name: &str) {
        wf_common::lock::lock_ok(self.cache.lock()).remove(server_name);
    }

    pub fn clear(&self) {
        wf_common::lock::lock_ok(self.cache.lock()).clear();
    }

    /// Remove expired entries; returns the number of entries removed.
    pub fn cleanup(&self) -> usize {
        let mut cache = wf_common::lock::lock_ok(self.cache.lock());
        let before = cache.len();
        cache.retain(|_, entry| !entry.is_expired());
        before - cache.len()
    }

    pub fn get_stats(&self) -> MetadataCacheStats {
        let mut cache = wf_common::lock::lock_ok(self.cache.lock());
        cache.retain(|_, entry| !entry.is_expired());
        let entries: Vec<&CacheEntry> = cache.values().collect();
        let size = entries.len();
        let total_tools: usize = entries.iter().map(|e| e.tools.len()).sum();
        MetadataCacheStats { size, total_tools }
    }
}

/// Snapshot of one server's cached metadata.
#[derive(Debug, Clone, Default)]
pub struct CachedServerMetadata {
    pub tools: Vec<McpToolInfo>,
    pub resources: Vec<wf_types::tool::McpResource>,
    pub resource_templates: Vec<wf_types::tool::McpResourceTemplate>,
    pub instructions: Option<String>,
}

/// Statistics about the cache contents.
#[derive(Debug, Clone, Default)]
pub struct MetadataCacheStats {
    pub size: usize,
    pub total_tools: usize,
}

/// Options controlling the generated MCP tools context. All fields default
/// to minimal content to avoid prompt pollution.
#[derive(Debug, Clone)]
pub struct McpToolsContextOptions {
    pub enabled: bool,
    pub tools_per_server: usize,
    pub hot_tools_limit: usize,
    pub include_server_status: bool,
    pub include_tool_descriptions: bool,
    pub compact_mode: bool,
    pub context_prefix: String,
    pub include_usage_hint: bool,
}

impl Default for McpToolsContextOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            tools_per_server: 5,
            hot_tools_limit: 0,
            include_server_status: false,
            include_tool_descriptions: true,
            compact_mode: false,
            context_prefix: "# Available MCP Tools".into(),
            include_usage_hint: true,
        }
    }
}

/// Result of [`McpToolsDynamicContextProvider::generate_context`].
#[derive(Debug, Clone, Default)]
pub struct GeneratedMcpToolsContext {
    pub has_servers: bool,
    pub content: String,
    pub server_count: usize,
    pub tool_count: usize,
    pub content_length: usize,
}

/// Generates a compact, customizable MCP tools summary for LLM prompts.
pub struct McpToolsDynamicContextProvider {
    manager: McpConnectionManager,
    analytics: Option<std::sync::Arc<McpUsageAnalytics>>,
}

impl McpToolsDynamicContextProvider {
    pub fn new(manager: McpConnectionManager) -> Self {
        Self {
            manager,
            analytics: None,
        }
    }

    pub fn with_analytics(mut self, analytics: std::sync::Arc<McpUsageAnalytics>) -> Self {
        self.analytics = Some(analytics);
        self
    }

    fn entry_status(entry: &McpServerEntry) -> &str {
        match entry.status {
            wf_types::tool::mcp_connection::McpServerStatus::Connected => "connected",
            wf_types::tool::mcp_connection::McpServerStatus::Connecting => "connecting",
            wf_types::tool::mcp_connection::McpServerStatus::Disconnected => "disconnected",
        }
    }

    pub fn generate_context(&self, options: &McpToolsContextOptions) -> GeneratedMcpToolsContext {
        if !options.enabled {
            return GeneratedMcpToolsContext::default();
        }

        let servers: Vec<McpServerEntry> = self.manager.registry().list();
        if servers.is_empty() {
            return GeneratedMcpToolsContext {
                has_servers: false,
                content: "No MCP servers are currently configured.".into(),
                server_count: 0,
                tool_count: 0,
                content_length: "No MCP servers are currently configured.".len(),
            };
        }

        let mut lines: Vec<String> = Vec::new();
        lines.push(options.context_prefix.clone());
        lines.push(String::new());

        let total_tools: usize = servers.iter().map(|s| s.tools.len()).sum();
        lines.push(format!(
            "{} server(s) with {} tool(s) available",
            servers.len(),
            total_tools
        ));
        lines.push(String::new());

        let mut server_count = 0usize;
        let mut tool_count = 0usize;
        for server in &servers {
            server_count += 1;
            lines.push(format!("## {}", server.name));

            if options.include_server_status
                && server.status != wf_types::tool::mcp_connection::McpServerStatus::Connected
            {
                lines.push(format!("Status: {}", Self::entry_status(server)));
            }

            if !server.tools.is_empty() {
                let tools_to_show = if options.tools_per_server == 0 {
                    &server.tools[..]
                } else {
                    &server.tools[..server.tools.len().min(options.tools_per_server)]
                };
                tool_count += tools_to_show.len();

                if options.compact_mode {
                    let names = tools_to_show
                        .iter()
                        .map(|t| format!("`{}`", t.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(names);
                } else {
                    for tool in tools_to_show {
                        if options.include_tool_descriptions {
                            if let Some(desc) = &tool.description {
                                lines.push(format!("- `{}` - {}", tool.name, desc));
                                continue;
                            }
                        }
                        lines.push(format!("- `{}`", tool.name));
                    }
                }

                if server.tools.len() > tools_to_show.len() {
                    lines.push(format!(
                        "*... and {} more*",
                        server.tools.len() - tools_to_show.len()
                    ));
                }
            }

            lines.push(String::new());
        }

        if options.hot_tools_limit > 0 {
            if let Some(analytics) = &self.analytics {
                let hot = analytics.get_hot_tools(options.hot_tools_limit);
                if !hot.is_empty() {
                    lines.push("## Recommended Tools".into());
                    for tool in hot {
                        let desc =
                            if options.include_tool_descriptions && !tool.tool_name.is_empty() {
                                format!(" - {}", tool.last_error.as_deref().unwrap_or(""))
                            } else {
                                String::new()
                            };
                        lines.push(format!(
                            "- `{}/{}`{}",
                            tool.server_name, tool.tool_name, desc
                        ));
                    }
                    lines.push(String::new());
                }
            }
        }

        if options.include_usage_hint {
            lines.push("Use: `use_mcp(server_name=\"...\", tool_name=\"...\")`".into());
        }

        let content = lines.join("\n");
        GeneratedMcpToolsContext {
            has_servers: true,
            content_length: content.len(),
            server_count,
            tool_count,
            content: content.clone(),
        }
    }

    pub fn get_context_stats(&self) -> ContextStats {
        let context = self.generate_context(&McpToolsContextOptions::default());
        let preview = context
            .content
            .lines()
            .take(5)
            .collect::<Vec<_>>()
            .join("\n");
        ContextStats {
            has_mcp_servers: context.has_servers,
            server_count: context.server_count,
            tool_count: context.tool_count,
            content_preview: format!("{}...", preview),
        }
    }
}

/// Statistics about the generated context.
#[derive(Debug, Clone, Default)]
pub struct ContextStats {
    pub has_mcp_servers: bool,
    pub server_count: usize,
    pub tool_count: usize,
    pub content_preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::connection::{McpConnectionManager, McpServerRegistry};
    use std::sync::Arc;
    use wf_types::tool::mcp_connection::*;

    fn make_stdio_config() -> McpServerConfig {
        McpServerConfig::Stdio(McpStdioConfig {
            base: McpServerConfigBase {
                disabled: None,
                timeout: Some(5),
                always_allow: None,
                disabled_tools: None,
                lifecycle: None,
                idle_timeout: None,
                health_check_interval: None,
            },
            command: "echo".into(),
            args: None,
            cwd: None,
            env: None,
        })
    }

    fn seed_server(registry: &McpServerRegistry, name: &str, tools: Vec<(&str, &str)>) {
        registry.register(name, make_stdio_config());
        let infos: Vec<McpToolInfo> = tools
            .into_iter()
            .map(|(n, d)| McpToolInfo {
                name: n.into(),
                description: Some(d.into()),
                input_schema: None,
            })
            .collect();
        registry.update_tools(name, infos);
        registry.update_status(name, McpServerStatus::Connected);
    }

    #[test]
    fn test_cache_set_get_expire() {
        let cache = McpToolMetadataCache::new(Duration::from_millis(50));
        let info = vec![McpToolInfo {
            name: "query".into(),
            description: None,
            input_schema: None,
        }];
        cache.set("db", info.clone(), vec![], vec![], None, None);
        assert!(cache.has("db"));
        let got = cache.get("db").unwrap();
        assert_eq!(got.tools.len(), 1);

        std::thread::sleep(Duration::from_millis(80));
        assert!(!cache.has("db"), "entry should expire after TTL");
        assert_eq!(cache.cleanup(), 0);
    }

    #[test]
    fn test_cache_invalidate_and_clear() {
        let cache = McpToolMetadataCache::new(Duration::from_secs(60));
        cache.set("a", vec![], vec![], vec![], None, None);
        cache.set("b", vec![], vec![], vec![], None, None);
        assert_eq!(cache.get_stats().size, 2);

        cache.invalidate("a");
        assert!(!cache.has("a"));
        assert!(cache.has("b"));

        cache.clear();
        assert_eq!(cache.get_stats().size, 0);
    }

    #[test]
    fn test_context_provider_empty() {
        let registry = Arc::new(McpServerRegistry::new());
        let manager = McpConnectionManager::new(registry);
        let provider = McpToolsDynamicContextProvider::new(manager);
        let ctx = provider.generate_context(&McpToolsContextOptions::default());
        assert!(!ctx.has_servers);
        assert!(ctx.content.contains("No MCP servers"));
    }

    #[test]
    fn test_context_provider_with_servers() {
        let registry = Arc::new(McpServerRegistry::new());
        seed_server(
            &registry,
            "db",
            vec![("query", "Run SQL"), ("list", "List tables")],
        );
        seed_server(&registry, "web", vec![("fetch", "Fetch URL")]);
        let manager = McpConnectionManager::new(registry);
        let provider = McpToolsDynamicContextProvider::new(manager);

        let ctx = provider.generate_context(&McpToolsContextOptions::default());
        assert!(ctx.has_servers);
        assert_eq!(ctx.server_count, 2);
        assert!(ctx.content.contains("2 server(s)"));
        assert!(ctx.content.contains("## db"));
        assert!(ctx.content.contains("Run SQL"));
        assert!(ctx.content.contains("use_mcp"));

        // Compact mode drops descriptions.
        let compact = McpToolsContextOptions {
            compact_mode: true,
            ..Default::default()
        };
        let ctx = provider.generate_context(&compact);
        assert!(!ctx.content.contains("Run SQL"));
        assert!(ctx.content.contains("`query`"));
    }

    #[test]
    fn test_context_disabled() {
        let registry = Arc::new(McpServerRegistry::new());
        seed_server(&registry, "db", vec![("query", "Run SQL")]);
        let manager = McpConnectionManager::new(registry);
        let provider = McpToolsDynamicContextProvider::new(manager);
        let opts = McpToolsContextOptions {
            enabled: false,
            ..Default::default()
        };
        let ctx = provider.generate_context(&opts);
        assert!(!ctx.has_servers);
        assert!(ctx.content.is_empty());
    }
}

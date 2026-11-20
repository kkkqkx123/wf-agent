use std::sync::Arc;

use super::config::McpRuntimeConfig;

pub async fn init_mcp(
    config: &McpRuntimeConfig,
) -> Option<Arc<wf_tools::mcp::connection::McpConnectionManager>> {
    use wf_tools::mcp::connection::{McpConnectionManager, McpServerRegistry};

    if config.settings_dir.is_none() && config.project_root.is_none() {
        return None;
    }
    let settings = wf_config::mcp::try_load_and_merge_mcp_settings(
        config.settings_dir.as_deref(),
        config.project_root.as_deref(),
    );
    if settings.mcp_servers.is_empty() {
        return None;
    }

    let registry = Arc::new(McpServerRegistry::new());
    let manager = Arc::new(McpConnectionManager::new(registry));
    for (name, server_config) in &settings.mcp_servers {
        if let Err(e) = manager.connect_server(name, server_config.clone()).await {
            tracing::warn!("MCP server '{}' failed to connect: {}", name, e);
        }
    }
    Some(manager)
}

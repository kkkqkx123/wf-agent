use std::sync::Arc;

use super::config::McpRuntimeConfig;

pub async fn init_mcp(
    config: &McpRuntimeConfig,
) -> Option<Arc<wf_tools::mcp::connection::McpConnectionManager>> {
    use wf_tools::mcp::connection::{McpConnectionManager, McpServerRegistry};

    let (Some(settings_dir), Some(project_root)) = (&config.settings_dir, &config.project_root)
    else {
        return None;
    };

    let settings =
        wf_config::mcp::load_and_merge_mcp_settings(settings_dir, project_root).unwrap_or_default();
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

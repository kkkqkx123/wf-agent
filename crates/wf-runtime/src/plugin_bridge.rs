use async_trait::async_trait;

use wf_plugin::{ContributionBridge, ContributionManager, PluginResult};

pub struct WfPluginBridge;

#[async_trait]
impl ContributionBridge for WfPluginBridge {
    async fn sync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        tracing::info!("[bridge] syncing contributions for '{}'", plugin_id);

        for (name, _) in manager.all_node_types() {
            tracing::debug!("  node-type: {}", name);
        }
        for (name, _) in manager.all_tool_types() {
            tracing::debug!("  tool-type: {}", name);
        }
        for (name, _) in manager.all_llm_providers() {
            tracing::debug!("  llm-provider: {}", name);
        }
        for (name, _) in manager.all_formatters() {
            tracing::debug!("  formatter: {}", name);
        }
        for (name, _) in manager.all_event_handlers() {
            tracing::debug!("  event-handler: {}", name);
        }
        for (name, _) in manager.all_hook_handlers() {
            tracing::debug!("  hook-handler: {}", name);
        }
        for phase in manager.all_middleware_phases() {
            tracing::debug!("  middleware: {}", phase);
        }

        Ok(())
    }

    async fn unsync_all(&self, plugin_id: &str, _manager: &ContributionManager) -> PluginResult<()> {
        tracing::info!("[bridge] unsyncing contributions for '{}'", plugin_id);
        Ok(())
    }
}

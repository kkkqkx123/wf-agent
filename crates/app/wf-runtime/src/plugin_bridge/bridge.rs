use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_plugin::{ContributionBridge, ContributionManager, PluginRegistry, PluginResult};
use wf_resource::registry::ResourceRegistries;
use wf_tools::registry::ToolRegistry;

use super::{advisors, auth, hooks, images, llm, resources, retrieval};

pub struct WfPluginBridge {
    registries: Arc<ResourceRegistries>,
    tool_registry: Arc<ToolRegistry>,
    llm_gateway: Arc<wf_llm::LlmGateway>,
    hook_registry: Arc<HookHandlerRegistry>,
    plugin_registry: Arc<PluginRegistry>,
}

impl WfPluginBridge {
    pub fn new(
        registries: Arc<ResourceRegistries>,
        tool_registry: Arc<ToolRegistry>,
        llm_gateway: Arc<wf_llm::LlmGateway>,
        hook_registry: Arc<HookHandlerRegistry>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> Self {
        Self {
            registries,
            tool_registry,
            llm_gateway,
            hook_registry,
            plugin_registry,
        }
    }
}

#[async_trait]
impl ContributionBridge for WfPluginBridge {
    async fn sync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        tracing::info!("[bridge] syncing contributions for '{}'", plugin_id);

        resources::log_behavioral_contributions(manager);
        resources::sync_tool_handlers(&self.tool_registry, plugin_id, manager);

        let bundle = resources::collect_bundle(plugin_id, manager);
        resources::install_resources(&self.registries, &self.tool_registry, &bundle, plugin_id);

        llm::sync_llm(&self.llm_gateway, plugin_id, manager)?;
        auth::sync_auth(&self.llm_gateway, plugin_id, manager)?;
        images::sync_images(plugin_id, manager);
        advisors::sync_advisors(plugin_id, manager)?;
        retrieval::sync_retrieval(plugin_id, manager)?;
        // Hook handlers resolve through the manifest mapping; registries
        // without a manifest (tests, minimal embedding) skip this step.
        if let Some(info) = self.plugin_registry.get(plugin_id) {
            hooks::sync_hooks(plugin_id, manager, &info.manifest, &self.hook_registry)?;
        }

        Ok(())
    }

    async fn unsync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        tracing::info!("[bridge] unsyncing contributions for '{}'", plugin_id);

        llm::unsync_llm(&self.llm_gateway, plugin_id, manager);
        auth::unsync_auth(&self.llm_gateway, plugin_id, manager);
        images::unsync_images(plugin_id, manager);
        advisors::unsync_advisors(plugin_id, manager);
        retrieval::unsync_retrieval(plugin_id, manager);
        hooks::unsync_hooks(plugin_id, manager, &self.hook_registry);

        let bundle = resources::collect_bundle(plugin_id, manager);
        resources::uninstall_resources(&self.registries, &self.tool_registry, &bundle);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::Registry;
    use wf_plugin::ContributionBridge;
    use wf_plugin::{
        ContributionRegistrar, PluginToolContext, PluginToolExecutor, PluginToolResult,
    };
    use wf_types::tool::{Tool, ToolType};

    struct EchoExecutor;

    #[async_trait]
    impl PluginToolExecutor for EchoExecutor {
        async fn execute(
            &self,
            ctx: PluginToolContext,
        ) -> wf_plugin::PluginResult<PluginToolResult> {
            Ok(PluginToolResult {
                result: serde_json::json!({"echo": ctx.args}),
            })
        }
    }

    fn contributed_tool(id: &str) -> Tool {
        Tool {
            id: id.into(),
            name: format!("{id} tool"),
            description: "plugin-contributed tool".into(),
            tool_type: ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: None,
            strict: None,
            default_timeout_ms: None,
        }
    }

    fn manager_with_tool(plugin_id: &str, type_name: &str, tool_id: &str) -> ContributionManager {
        let manager = ContributionManager::new();
        manager.start_registration(plugin_id);
        manager
            .as_registrar()
            .register_tool_type(type_name, Arc::new(EchoExecutor))
            .unwrap();
        manager
            .as_registrar()
            .register_tool(tool_id, contributed_tool(tool_id))
            .unwrap();
        manager
    }

    #[tokio::test]
    async fn sync_install_tool_and_handler_unsync_removes_both() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = Arc::new(wf_llm::LlmGateway::new());
        let bridge = WfPluginBridge::new(
            registries,
            tool_registry.clone(),
            gateway.clone(),
            Arc::new(wf_execution_shared::hooks::HookHandlerRegistry::new()),
            Arc::new(wf_plugin::PluginRegistry::new()),
        );
        let manager = manager_with_tool("p1", "echo_type", "p1.echo");

        bridge.sync_all("p1", &manager).await.unwrap();
        assert!(tool_registry.has("p1.echo"), "tool registered on sync");

        bridge.unsync_all("p1", &manager).await.unwrap();
        assert!(
            !tool_registry.has("p1.echo"),
            "tool removed on unsync (symmetric teardown)"
        );
    }

    #[tokio::test]
    async fn unsync_only_touches_owning_plugin() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = Arc::new(wf_llm::LlmGateway::new());
        let bridge = WfPluginBridge::new(
            registries,
            tool_registry.clone(),
            gateway.clone(),
            Arc::new(wf_execution_shared::hooks::HookHandlerRegistry::new()),
            Arc::new(wf_plugin::PluginRegistry::new()),
        );

        let m1 = manager_with_tool("p1", "t1", "p1.a");
        let m2 = manager_with_tool("p2", "t2", "p2.b");
        bridge.sync_all("p1", &m1).await.unwrap();
        bridge.sync_all("p2", &m2).await.unwrap();
        assert!(tool_registry.has("p1.a"));
        assert!(tool_registry.has("p2.b"));

        bridge.unsync_all("p1", &m1).await.unwrap();
        assert!(!tool_registry.has("p1.a"), "p1 tool removed");
        assert!(tool_registry.has("p2.b"), "p2 tool untouched");
    }
}

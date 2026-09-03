use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_types::enums::MiddlewarePhase;

use wf_api::infra::handler_chain::{
    PluginHandlerSource, PluginMiddlewareBridge, PluginNodeExecutor,
};
use wf_core::registry::{MutableRegistry, Registry};
use wf_plugin::{ContributionBridge, ContributionManager, PluginResult};
use wf_resource::registry::ResourceRegistries;
use wf_tools::registry::ToolRegistry;

/// Bridge between the `wf-plugin` contribution manager and the runtime's
/// declarative resource registries (`ResourceRegistries` + `ToolRegistry`).
///
/// Behavioral contributions (node/tool/llm/middleware) stay on the execution
/// path (consumed by `WfPluginHandlerSource`); this bridge is the single
/// landing point for **declarative resource contributions**
/// (workflow/prompt/fragment/agent/node/trigger/tool-description/tool):
/// `sync_all` writes them into `ResourceRegistries` / `ToolRegistry` on
/// activation, `unsync_all` removes them symmetrically on deactivation.
pub struct WfPluginBridge {
    registries: Arc<ResourceRegistries>,
    tool_registry: Arc<ToolRegistry>,
}

impl WfPluginBridge {
    pub fn new(registries: Arc<ResourceRegistries>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            registries,
            tool_registry,
        }
    }
}

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
        for phase in manager.all_middleware_phases() {
            tracing::debug!("  middleware: {}", phase);
        }

        // Declarative resource contribution placement (skip-existing, idempotent)
        for (id, owner) in manager.all_workflows() {
            if owner == plugin_id {
                if let Some(wf) = manager.get_workflow(&id) {
                    wf_resource::register_item_skip(
                        &self.registries.workflows,
                        id.clone(),
                        (*wf).clone(),
                    );
                    tracing::debug!("  workflow: {}", id);
                }
            }
        }
        for (id, owner) in manager.all_prompts() {
            if owner == plugin_id {
                if let Some(t) = manager.get_prompt(&id) {
                    wf_resource::register_template(&self.registries, (*t).clone(), true);
                    tracing::debug!("  prompt: {}", id);
                }
            }
        }
        for (id, owner) in manager.all_fragments() {
            if owner == plugin_id {
                if let Some(f) = manager.get_fragment(&id) {
                    wf_resource::register_fragment(&self.registries, (*f).clone(), true);
                    tracing::debug!("  fragment: {}", id);
                }
            }
        }
        for (id, owner) in manager.all_agent_templates() {
            if owner == plugin_id {
                if let Some(a) = manager.get_agent_template(&id) {
                    wf_resource::register_item_skip(
                        &self.registries.agent_templates,
                        id.clone(),
                        (*a).clone(),
                    );
                    tracing::debug!("  agent-template: {}", id);
                }
            }
        }
        for (id, owner) in manager.all_node_templates() {
            if owner == plugin_id {
                if let Some(n) = manager.get_node_template(&id) {
                    wf_resource::register_item_skip(
                        &self.registries.node_templates,
                        id.clone(),
                        (*n).clone(),
                    );
                    tracing::debug!("  node-template: {}", id);
                }
            }
        }
        for (id, owner) in manager.all_triggers() {
            if owner == plugin_id {
                if let Some(t) = manager.get_trigger(&id) {
                    wf_resource::register_item_skip(
                        &self.registries.trigger_templates,
                        t.name.clone(),
                        (*t).clone(),
                    );
                    tracing::debug!("  trigger: {}", t.name);
                }
            }
        }
        for (id, owner) in manager.all_tool_descriptions() {
            if owner == plugin_id {
                if let Some(d) = manager.get_tool_description(&id) {
                    wf_resource::register_item_skip(
                        &self.registries.tool_descriptions,
                        id.clone(),
                        (*d).clone(),
                    );
                    tracing::debug!("  tool-description: {}", id);
                }
            }
        }
        for (id, owner) in manager.all_tools() {
            if owner == plugin_id {
                if let Some(tool) = manager.get_tool(&id) {
                    if !self.tool_registry.has(&tool.id) {
                        self.tool_registry.register_tool((*tool).clone());
                        tracing::debug!("  tool: {}", tool.id);
                    }
                }
            }
        }

        Ok(())
    }

    async fn unsync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        tracing::info!("[bridge] unsyncing contributions for '{}'", plugin_id);

        for (id, owner) in manager.all_workflows() {
            if owner == plugin_id {
                self.registries.workflows.unregister(&id);
            }
        }
        for (id, owner) in manager.all_prompts() {
            if owner == plugin_id {
                self.registries.templates.unregister(&id);
            }
        }
        for (id, owner) in manager.all_fragments() {
            if owner == plugin_id {
                self.registries.fragments.unregister(&id);
            }
        }
        for (id, owner) in manager.all_agent_templates() {
            if owner == plugin_id {
                self.registries.agent_templates.unregister(&id);
            }
        }
        for (id, owner) in manager.all_node_templates() {
            if owner == plugin_id {
                self.registries.node_templates.unregister(&id);
            }
        }
        for (id, owner) in manager.all_triggers() {
            if owner == plugin_id {
                if let Some(t) = manager.get_trigger(&id) {
                    self.registries.trigger_templates.unregister(&t.name);
                }
            }
        }
        for (id, owner) in manager.all_tool_descriptions() {
            if owner == plugin_id {
                self.registries.tool_descriptions.unregister(&id);
            }
        }
        for (id, owner) in manager.all_tools() {
            if owner == plugin_id {
                self.tool_registry.remove_tool(&id);
            }
        }

        Ok(())
    }
}

/// `wf-plugin` contribution source wired into `ApiContext`'s handler
/// resolution chain (builtin → plugin → template fallback). Translates plugin
/// contribution traits onto the plugin-agnostic bridge traits of `wf-api`.
pub struct WfPluginHandlerSource {
    manager: Arc<ContributionManager>,
}

impl WfPluginHandlerSource {
    pub fn new(manager: Arc<ContributionManager>) -> Self {
        Self { manager }
    }
}

impl PluginHandlerSource for WfPluginHandlerSource {
    fn node_executor(&self, type_name: &str) -> Option<Arc<dyn PluginNodeExecutor>> {
        self.manager
            .get_node_handler(type_name)
            .map(|handler| Arc::new(WfPluginNodeExecutor(handler)) as Arc<dyn PluginNodeExecutor>)
    }

    fn middleware(&self, phase: &MiddlewarePhase) -> Vec<Arc<dyn PluginMiddlewareBridge>> {
        // The plugin engine chains middleware through a `next` closure; the
        // manager exposes the fully-chained run, so surface it as a single
        // bridge to avoid running the chain once per handler.
        if self.manager.get_middleware(phase).is_empty() {
            return Vec::new();
        }
        vec![Arc::new(WfPluginMiddlewareRunner(self.manager.clone()))
            as Arc<dyn PluginMiddlewareBridge>]
    }
}

struct WfPluginNodeExecutor(Arc<dyn wf_plugin::PluginNodeHandler>);

#[async_trait]
impl PluginNodeExecutor for WfPluginNodeExecutor {
    async fn execute(
        &self,
        node_id: &str,
        inputs: &Value,
        config: &Value,
    ) -> wf_api::ApiResult<Value> {
        let ctx = wf_plugin::PluginExecutionContext {
            node_id: node_id.to_string(),
            inputs: inputs.clone(),
            config: config.clone(),
        };
        let result = self
            .0
            .execute(ctx)
            .await
            .map_err(wf_api::ApiError::execution_with_source)?;
        Ok(result.outputs)
    }
}

struct WfPluginMiddlewareRunner(Arc<ContributionManager>);

#[async_trait]
impl PluginMiddlewareBridge for WfPluginMiddlewareRunner {
    async fn handle(&self, phase: &MiddlewarePhase, context: &Value) -> wf_api::ApiResult<()> {
        self.0
            .run_middleware(phase, context.clone())
            .await
            .map_err(wf_api::ApiError::execution_with_source)
    }
}

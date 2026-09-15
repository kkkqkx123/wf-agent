use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_types::enums::MiddlewarePhase;

use wf_api::infra::handler_chain::{
    PluginHandlerSource, PluginMiddlewareBridge, PluginNodeExecutor,
};
use wf_core::registry::{MutableRegistry, Registry};
use wf_plugin::{ContributionBridge, ContributionManager, PluginError, PluginResult};
use wf_resource::registry::ResourceRegistries;
use wf_tools::error::ToolResult;
use wf_tools::executor::trait_def::ToolExecutionContext;
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
    llm_gateway: Arc<wf_llm::LlmGateway>,
}

impl WfPluginBridge {
    pub fn new(
        registries: Arc<ResourceRegistries>,
        tool_registry: Arc<ToolRegistry>,
        llm_gateway: Arc<wf_llm::LlmGateway>,
    ) -> Self {
        Self {
            registries,
            tool_registry,
            llm_gateway,
        }
    }

    /// Sync plugin LLM contributions into the gateway: wrap each owned
    /// codec in a `PluginCodecAdapter` and register it as a custom format,
    /// then write each owned provider definition into the provider
    /// registry (which evicts cached clients).
    fn sync_llm(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        for (name, owner) in manager.all_llm_providers() {
            if owner != plugin_id {
                continue;
            }
            let Some(codec) = manager.get_llm_codec(&name) else {
                continue;
            };
            let adapter = Arc::new(wf_llm::PluginCodecAdapter::new(codec));
            if let Err(e) = self
                .llm_gateway
                .codec_registry()
                .register(&name, adapter)
            {
                tracing::warn!(
                    plugin_id,
                    format = %name,
                    "plugin llm codec registration failed: {e}"
                );
                return Err(PluginError::LoadFailed(format!(
                    "plugin '{plugin_id}' codec '{name}': {e}"
                )));
            }
            tracing::debug!("  llm-codec (gateway-registered): {}", name);
        }
        for (id, owner) in manager.all_llm_provider_definitions() {
            if owner != plugin_id {
                continue;
            }
            let Some(definition) = manager.get_llm_provider_definition(&id) else {
                continue;
            };
            self.llm_gateway
                .register_provider_definition((*definition).clone())
                .map_err(|e| {
                    PluginError::LoadFailed(format!("plugin '{plugin_id}' provider '{id}': {e}"))
                })?;
            tracing::debug!("  llm-provider-definition (gateway-registered): {}", id);
        }
        Ok(())
    }

    /// Symmetric teardown of [`Self::sync_llm`]: unregister the plugin's
    /// codecs and remove its provider definitions (both evict cached
    /// gateway clients).
    fn unsync_llm(&self, plugin_id: &str, manager: &ContributionManager) {
        for (name, owner) in manager.all_llm_providers() {
            if owner == plugin_id {
                self.llm_gateway.codec_registry().unregister(&name);
                tracing::debug!("  llm-codec (gateway-removed): {}", name);
            }
        }
        for (id, owner) in manager.all_llm_provider_definitions() {
            if owner == plugin_id {
                self.llm_gateway.remove_provider_definition(&id);
                tracing::debug!("  llm-provider-definition (gateway-removed): {}", id);
            }
        }
    }
}

#[async_trait]
impl ContributionBridge for WfPluginBridge {
    async fn sync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        tracing::info!("[bridge] syncing contributions for '{}'", plugin_id);

        // Behavioral contributions are resolved on the execution path:
        // node executors and middleware through `WfPluginHandlerSource`
        // (handler chain builtin → plugin → template), tool-type executors
        // land here as stateless async handlers so `ToolRegistry` can
        // dispatch them. LLM codecs and provider definitions sync into the
        // gateway below (`sync_llm`): each owned codec is wrapped in a
        // `PluginCodecAdapter` and registered as a custom format, each
        // owned provider definition is written into the provider registry
        // (evicting cached gateway clients).
        for (name, _) in manager.all_node_types() {
            tracing::debug!("  node-type: {}", name);
        }
        for (name, _) in manager.all_event_handlers() {
            tracing::debug!("  event-handler: {}", name);
        }
        for phase in manager.all_middleware_phases() {
            tracing::debug!("  middleware: {}", phase);
        }

        // Tool-type executors → stateless async handlers. The host
        // `ToolType` enum is closed, so a plugin's custom tool type cannot
        // be expressed on `Tool::tool_type`; the convention is that a
        // plugin's tool-type executor serves the tools contributed by the
        // same plugin (handlers keyed by tool id, resolved by
        // `StatelessExecutor` through `tool.id`).
        for (type_name, _) in manager.all_tool_types() {
            if let Some(executor) = manager.get_tool_executor(&type_name) {
                let handler = make_plugin_tool_handler(executor.clone());
                for (id, owner) in manager.all_tools() {
                    if owner == plugin_id {
                        self.tool_registry
                            .register_stateless_async_handler(&id, handler.clone());
                        tracing::debug!("  tool-type: {} → handler for '{}'", type_name, id);
                    }
                }
            }
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

        // LLM formats and providers land in the gateway last so a codec
        // registration failure fails activation loudly instead of running
        // with missing formats.
        self.sync_llm(plugin_id, manager)?;

        Ok(())
    }

    async fn unsync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        tracing::info!("[bridge] unsyncing contributions for '{}'", plugin_id);

        // LLM teardown first while the manager still holds the plugin's
        // entries (mirrors the tail of `sync_all`).
        self.unsync_llm(plugin_id, manager);

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
                // Symmetric teardown of the tool-type handler installed by
                // `sync_all` (see the tool-type bridge comment there).
                self.tool_registry.unregister_stateless_handler(&id);
            }
        }

        Ok(())
    }
}

/// Adapt a plugin `PluginToolExecutor` to a `ToolRegistry` stateless async
/// handler. Tool execution errors are reported as failed tool results
/// (carrying the plugin error message) rather than aborting the caller.
fn make_plugin_tool_handler(
    executor: Arc<dyn wf_plugin::PluginToolExecutor>,
) -> wf_tools::executor::StatelessAsyncHandler {
    Arc::new(move |args: Value, _ctx: ToolExecutionContext| {
        let executor = executor.clone();
        Box::pin(async move {
            executor
                .execute(wf_plugin::PluginToolContext { args })
                .await
                .map(|result| result.result)
                .map_err(|e| wf_tools::ToolError::ExecutionFailed {
                    tool_id: "plugin-tool".to_string(),
                    reason: e.to_string(),
                })
        }) as Pin<Box<dyn Future<Output = ToolResult<Value>> + Send>>
    })
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

    fn plugin_node_types(&self) -> Vec<String> {
        self.manager
            .all_node_types()
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    fn llm_provider_names(&self) -> Vec<String> {
        self.manager
            .all_llm_providers()
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    fn event_handler_event_types(&self) -> Vec<String> {
        self.manager
            .all_event_handlers()
            .into_iter()
            .map(|(event_type, _)| event_type)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
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
        // The chain may rewrite the context, but this bridge has no upstream
        // to hand the rewritten value to, so the final value is discarded.
        let _ = self
            .0
            .run_middleware(phase, context.clone())
            .await
            .map_err(wf_api::ApiError::execution_with_source)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let bridge = WfPluginBridge::new(registries, tool_registry.clone(), gateway.clone());
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
        let bridge = WfPluginBridge::new(registries, tool_registry.clone(), gateway.clone());

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

    struct NoopCodec;

    impl wf_plugin::PluginLlmCodec for NoopCodec {
        fn build_request(
            &self,
            _request: serde_json::Value,
            _profile: serde_json::Value,
        ) -> wf_plugin::PluginResult<wf_plugin::CodecHttpRequest> {
            Err(wf_plugin::PluginError::Internal("noop".to_string()))
        }

        fn parse_response(
            &self,
            _body: &str,
            _request: serde_json::Value,
        ) -> wf_plugin::PluginResult<serde_json::Value> {
            Err(wf_plugin::PluginError::Internal("noop".to_string()))
        }
    }

    #[tokio::test]
    async fn sync_registers_codec_and_provider_unsync_removes_both() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = Arc::new(wf_llm::LlmGateway::new());
        let bridge = WfPluginBridge::new(registries, tool_registry, gateway.clone());

        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_llm_provider("acme-codec", Arc::new(NoopCodec))
            .unwrap();
        manager
            .as_registrar()
            .register_llm_provider_definition(wf_types::llm::LlmProviderDefinition {
                id: "acme".to_string(),
                name: None,
                description: None,
                base_url: Some("https://api.acme.test".to_string()),
                auth_type: Some("bearer".to_string()),
                default_headers: None,
                format: "ACME_CODEC".to_string(),
                model_discovery: None,
                api_version: None,
                metadata: None,
            })
            .unwrap();

        bridge.sync_all("p1", &manager).await.unwrap();
        assert!(
            gateway.codec_registry().contains("acme-codec"),
            "codec registered as custom format"
        );
        assert!(
            gateway.provider_registry().has("acme"),
            "provider definition written to provider registry"
        );

        bridge.unsync_all("p1", &manager).await.unwrap();
        assert!(
            !gateway.codec_registry().contains("acme-codec"),
            "codec removed on unsync"
        );
        assert!(
            !gateway.provider_registry().has("acme"),
            "provider definition removed on unsync"
        );
    }
}

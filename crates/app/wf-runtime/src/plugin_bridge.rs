use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_types::enums::MiddlewarePhase;

use wf_api::infra::handler_chain::{
    PluginHandlerSource, PluginMiddlewareBridge, PluginNodeExecutor,
};
use wf_core::Registry;
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
            if let Err(e) = self.llm_gateway.codec_registry().register(&name, adapter) {
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

    /// Sync plugin auth contributions into the gateway's auth registry:
    /// wrap each owned `PluginLlmAuth` in a `PluginAuthAdapter` and register
    /// it under the auth name — the credential owner and login entry point
    /// profiles reference through their `auth` section. Declared parameters
    /// come from the provider definition referencing the auth (sorted
    /// provider-id order wins when several do; sharing an auth requires
    /// identical params). The adapter is OAuth-only: plugin login always
    /// produces an OAuth credential, API-key paste stays on built-in
    /// surfaces.
    ///
    /// Then verify every owned provider definition that declares an auth
    /// reference resolves to one of this plugin's registered auths: a
    /// dangling reference would surface as a per-request failure, so it
    /// fails activation loudly instead.
    fn sync_auth(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        let registry = self.llm_gateway.auth_registry();
        let mut registered: Vec<String> = Vec::new();
        for (name, owner) in manager.all_llm_auths() {
            if owner != plugin_id {
                continue;
            }
            let Some(auth) = manager.get_llm_auth(&name) else {
                continue;
            };
            let Some(registry) = registry else {
                tracing::warn!(
                    plugin_id,
                    auth = %name,
                    "gateway has no auth registry; plugin llm auth not registered"
                );
                continue;
            };
            let params = auth_params_for(plugin_id, manager, &name);
            let adapter = Arc::new(wf_llm::PluginAuthAdapter::new(auth, params));
            registry.register_provider(&name, wf_llm::ProviderAuth::oauth_only(adapter));
            // A replaced surface may resolve differently; drop clients that
            // resolved through the previous registration.
            self.llm_gateway.evict_clients_for_auth(&name);
            registered.push(name.clone());
            tracing::debug!("  llm-auth (gateway-registered): {}", name);
        }
        for (id, owner) in manager.all_llm_provider_definitions() {
            if owner != plugin_id {
                continue;
            }
            let Some(definition) = manager.get_llm_provider_definition(&id) else {
                continue;
            };
            if let Some(auth_ref) = definition.auth.as_ref() {
                if !registered.contains(&auth_ref.provider) {
                    return Err(PluginError::ActivationFailed(format!(
                        "plugin '{plugin_id}' provider '{id}' references llm auth '{}' \
                         which the plugin does not register",
                        auth_ref.provider
                    )));
                }
            }
        }
        Ok(())
    }

    /// Sync plugin image contributions: image providers have no gateway
    /// registry (the gateway builds a short-lived OpenAI-compatible
    /// transport per `generate_images` call), so this validates presence
    /// and logs ownership. Hosts consume a plugin images implementation
    /// explicitly through `wf_llm::PluginImagesAdapter` with their own
    /// `ImagesContext`; network implementations must run on the native
    /// backend like `llm-auth` (Lua/Wasm sandboxes have no HTTP).
    fn sync_images(&self, plugin_id: &str, manager: &ContributionManager) {
        for (name, owner) in manager.all_llm_images() {
            if owner == plugin_id {
                tracing::debug!("  llm-images (plugin-owned): {}", name);
            }
        }
    }

    /// Symmetric teardown of [`Self::sync_images`]: image providers hold no
    /// gateway state, so teardown only logs the removal.
    fn unsync_images(&self, plugin_id: &str, manager: &ContributionManager) {
        for (name, owner) in manager.all_llm_images() {
            if owner == plugin_id {
                tracing::debug!("  llm-images (plugin-removed): {}", name);
            }
        }
    }

    /// Symmetric teardown of [`Self::sync_auth`]: remove the plugin's auth
    /// surfaces and evict the clients that resolved through them.
    fn unsync_auth(&self, plugin_id: &str, manager: &ContributionManager) {
        let Some(registry) = self.llm_gateway.auth_registry() else {
            return;
        };
        for (name, owner) in manager.all_llm_auths() {
            if owner == plugin_id {
                registry.remove_provider(&name);
                self.llm_gateway.evict_clients_for_auth(&name);
                tracing::debug!("  llm-auth (gateway-removed): {}", name);
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
        let owned_tools: Vec<String> = manager
            .all_tools()
            .into_iter()
            .filter_map(|(id, owner)| (owner == plugin_id).then_some(id))
            .collect();
        let owned_types: Vec<String> = manager
            .all_tool_types()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        if !owned_types.is_empty() && owned_tools.is_empty() {
            tracing::warn!(
                plugin_id,
                tool_types = ?owned_types,
                "plugin registers tool-type executors but no declarative tools; handlers will not install"
            );
        }
        if !owned_tools.is_empty()
            && manager
                .all_tool_types()
                .into_iter()
                .all(|(_, owner)| owner != plugin_id)
        {
            tracing::warn!(
                plugin_id,
                tools = ?owned_tools,
                "plugin contributes tools without a paired tool-type executor; calls will resolve to NotFound until an executor is registered"
            );
        }
        if owned_types.len() > 1 && !owned_tools.is_empty() {
            tracing::warn!(
                plugin_id,
                tool_types = ?owned_types,
                tools = ?owned_tools,
                "multiple tool-type executors share one plugin tool set; last registration wins per tool id"
            );
        }
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

        // Declarative resources land through the single `install_bundle`
        // helper shared with direct installation, so validation and
        // skip-existing semantics stay identical. Rejections are reported
        // loudly but do not fail activation, matching the `Ok(Summary)`
        // semantics.
        let mut bundle = wf_resource::resource_assembler::ResourceBundle::new();
        for (id, owner) in manager.all_workflows() {
            if owner == plugin_id {
                if let Some(wf) = manager.get_workflow(&id) {
                    bundle.workflows.push((*wf).clone());
                }
            }
        }
        for (id, owner) in manager.all_prompts() {
            if owner == plugin_id {
                if let Some(t) = manager.get_prompt(&id) {
                    bundle.prompts.push((*t).clone());
                }
            }
        }
        for (id, owner) in manager.all_fragments() {
            if owner == plugin_id {
                if let Some(f) = manager.get_fragment(&id) {
                    bundle.fragments.push((*f).clone());
                }
            }
        }
        for (id, owner) in manager.all_agent_templates() {
            if owner == plugin_id {
                if let Some(a) = manager.get_agent_template(&id) {
                    bundle.agent_templates.push((*a).clone());
                }
            }
        }
        for (id, owner) in manager.all_node_templates() {
            if owner == plugin_id {
                if let Some(n) = manager.get_node_template(&id) {
                    bundle.node_templates.push((*n).clone());
                }
            }
        }
        for (id, owner) in manager.all_triggers() {
            if owner == plugin_id {
                if let Some(t) = manager.get_trigger(&id) {
                    bundle.triggers.push((*t).clone());
                }
            }
        }
        for (id, owner) in manager.all_tool_descriptions() {
            if owner == plugin_id {
                if let Some(d) = manager.get_tool_description(&id) {
                    bundle.tool_descriptions.push((*d).clone());
                }
            }
        }
        for (id, owner) in manager.all_tools() {
            if owner == plugin_id {
                if let Some(tool) = manager.get_tool(&id) {
                    if self.tool_registry.has(&tool.id) {
                        tracing::warn!(
                            plugin_id,
                            tool = %tool.id,
                            "plugin tool id collides with a registered tool; install_bundle will skip it"
                        );
                    }
                    bundle.tools.push((*tool).clone());
                }
            }
        }
        let summary = wf_resource::resource_assembler::install_bundle(
            &self.registries,
            &self.tool_registry,
            &bundle,
            true,
        );
        for id in &summary.succeeded {
            tracing::debug!("  resource: {}", id);
        }
        for fail in &summary.failed {
            tracing::warn!(
                plugin_id,
                resource = %fail.id,
                "plugin resource registration failed: {}",
                fail.error
            );
        }

        // LLM formats and providers land in the gateway last so a codec
        // registration failure fails activation loudly instead of running
        // with missing formats. Auth surfaces follow: they key the
        // credential store and the login entry points. Image contributions
        // hold no gateway state; they are validated and logged so ownership
        // stays visible in activation traces.
        self.sync_llm(plugin_id, manager)?;
        self.sync_auth(plugin_id, manager)?;
        self.sync_images(plugin_id, manager);

        Ok(())
    }

    async fn unsync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
        tracing::info!("[bridge] unsyncing contributions for '{}'", plugin_id);

        // LLM teardown first while the manager still holds the plugin's
        // entries (mirrors the tail of `sync_all`).
        self.unsync_llm(plugin_id, manager);
        self.unsync_auth(plugin_id, manager);
        self.unsync_images(plugin_id, manager);

        // Symmetric teardown of `sync_all` through the shared
        // `uninstall_bundle` helper.
        let mut bundle = wf_resource::resource_assembler::ResourceBundle::new();
        for (id, owner) in manager.all_workflows() {
            if owner == plugin_id {
                if let Some(wf) = manager.get_workflow(&id) {
                    bundle.workflows.push((*wf).clone());
                }
            }
        }
        for (id, owner) in manager.all_prompts() {
            if owner == plugin_id {
                if let Some(t) = manager.get_prompt(&id) {
                    bundle.prompts.push((*t).clone());
                }
            }
        }
        for (id, owner) in manager.all_fragments() {
            if owner == plugin_id {
                if let Some(f) = manager.get_fragment(&id) {
                    bundle.fragments.push((*f).clone());
                }
            }
        }
        for (id, owner) in manager.all_agent_templates() {
            if owner == plugin_id {
                if let Some(a) = manager.get_agent_template(&id) {
                    bundle.agent_templates.push((*a).clone());
                }
            }
        }
        for (id, owner) in manager.all_node_templates() {
            if owner == plugin_id {
                if let Some(n) = manager.get_node_template(&id) {
                    bundle.node_templates.push((*n).clone());
                }
            }
        }
        for (id, owner) in manager.all_triggers() {
            if owner == plugin_id {
                if let Some(t) = manager.get_trigger(&id) {
                    bundle.triggers.push((*t).clone());
                }
            }
        }
        for (id, owner) in manager.all_tool_descriptions() {
            if owner == plugin_id {
                if let Some(d) = manager.get_tool_description(&id) {
                    bundle.tool_descriptions.push((*d).clone());
                }
            }
        }
        for (id, owner) in manager.all_tools() {
            if owner == plugin_id {
                if let Some(tool) = manager.get_tool(&id) {
                    bundle.tools.push((*tool).clone());
                }
            }
        }
        wf_resource::resource_assembler::uninstall_bundle(
            &self.registries,
            &self.tool_registry,
            &bundle,
        );
        for tool in &bundle.tools {
            // Symmetric teardown of the tool-type handler installed by
            // `sync_all` (see the tool-type bridge comment there).
            self.tool_registry.unregister_stateless_handler(&tool.id);
        }

        Ok(())
    }
}

/// Declared auth parameters for one auth name as a JSON object: taken from
/// the provider definition owned by the same plugin that references it.
/// Definitions are visited in sorted provider-id order so the winner is
/// deterministic when several reference the same auth. Sharing an auth name
/// means sharing one credential owner, so all referencing definitions must
/// carry identical params; the first sorted winner is used and divergence
/// is a declaration error the plugin author must fix.
fn auth_params_for(
    plugin_id: &str,
    manager: &ContributionManager,
    auth_name: &str,
) -> Option<Value> {
    let mut definitions: Vec<_> = manager.all_llm_provider_definitions();
    definitions.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, owner) in definitions {
        if owner != plugin_id {
            continue;
        }
        let Some(definition) = manager.get_llm_provider_definition(&id) else {
            continue;
        };
        if let Some(auth_ref) = definition.auth.as_ref() {
            if auth_ref.provider == auth_name {
                return auth_ref.params.as_ref().map(|params| {
                    Value::Object(params.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                });
            }
        }
    }
    None
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
    use wf_core::registry::Registry;
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
                format: wf_types::llm::LlmFormat::Custom("ACME_CODEC".to_string()),
                model_discovery: None,
                api_version: None,
                metadata: None,
                proxy: None,
                no_proxy: None,
                rate_limit: None,
                auth: None,
                compat: None,
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

    struct StubAuth;

    impl wf_plugin::PluginLlmAuth for StubAuth {
        fn describe(&self) -> wf_plugin::PluginResult<wf_plugin::AuthDescriptor> {
            Ok(wf_plugin::AuthDescriptor {
                name: "Example Sub".to_string(),
                is_subscription: true,
                login_label: Some("Sign in with Example Sub".to_string()),
            })
        }

        fn login_start(
            &self,
            _config: serde_json::Value,
        ) -> wf_plugin::PluginResult<wf_plugin::PluginLoginChallenge> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }

        fn login_poll(
            &self,
            _session: serde_json::Value,
        ) -> wf_plugin::PluginResult<wf_plugin::PluginLoginStep> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }

        fn login_submit(
            &self,
            _session: serde_json::Value,
            _input: String,
        ) -> wf_plugin::PluginResult<wf_plugin::PluginLoginStep> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }

        fn refresh(
            &self,
            _credential: serde_json::Value,
        ) -> wf_plugin::PluginResult<serde_json::Value> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }

        fn resolve(
            &self,
            _credential: serde_json::Value,
        ) -> wf_plugin::PluginResult<serde_json::Value> {
            Err(wf_plugin::PluginError::Internal("stub".to_string()))
        }
    }

    fn gateway_with_auth_registry() -> Arc<wf_llm::LlmGateway> {
        let store = Arc::new(wf_llm::llm_auth::MemoryCredentialStore::new());
        let registry =
            wf_llm::llm_auth::AuthRegistry::new(store, Arc::new(wf_llm::llm_auth::StdAuthContext));
        Arc::new(wf_llm::LlmGateway::new().with_auth(Arc::new(registry)))
    }

    fn provider_definition_with_auth(
        id: &str,
        auth_provider: &str,
    ) -> wf_types::llm::LlmProviderDefinition {
        wf_types::llm::LlmProviderDefinition {
            id: id.to_string(),
            name: None,
            description: None,
            base_url: Some("https://api.acme.test".to_string()),
            auth_type: Some("bearer".to_string()),
            default_headers: None,
            format: wf_types::llm::LlmFormat::Anthropic,
            model_discovery: None,
            api_version: None,
            metadata: None,
            proxy: None,
            no_proxy: None,
            rate_limit: None,
            auth: Some(wf_types::llm::ProviderAuthRef {
                provider: auth_provider.to_string(),
                method: Some(wf_types::llm::AuthMethod::OAuth),
                is_subscription: Some(true),
                params: Some(
                    [("flow".to_string(), serde_json::json!("pkce_paste"))]
                        .into_iter()
                        .collect(),
                ),
            }),
            compat: None,
        }
    }

    #[tokio::test]
    async fn sync_registers_plugin_auth_unsync_removes_it() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = gateway_with_auth_registry();
        let bridge = WfPluginBridge::new(registries, tool_registry, gateway.clone());

        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_llm_auth("claude-subscription", Arc::new(StubAuth))
            .unwrap();
        manager
            .as_registrar()
            .register_llm_provider_definition(provider_definition_with_auth(
                "claude-sub",
                "claude-subscription",
            ))
            .unwrap();

        bridge.sync_all("p1", &manager).await.unwrap();
        let registry = gateway.auth_registry().expect("auth registry attached");
        assert!(
            registry.has_provider("claude-subscription"),
            "plugin auth registered under its contribution name"
        );
        let surfaces = registry.describe_providers();
        let surface = surfaces
            .iter()
            .find(|surface| surface.id == "claude-subscription")
            .expect("auth surface described");
        assert_eq!(surface.display_name, "Example Sub");
        assert!(surface.is_subscription);

        bridge.unsync_all("p1", &manager).await.unwrap();
        assert!(
            !registry.has_provider("claude-subscription"),
            "auth surface removed on unsync"
        );
    }

    #[tokio::test]
    async fn sync_fails_when_provider_references_unregistered_auth() {
        let registries = Arc::new(ResourceRegistries::new());
        let tool_registry = Arc::new(ToolRegistry::new());
        let gateway = gateway_with_auth_registry();
        let bridge = WfPluginBridge::new(registries, tool_registry, gateway.clone());

        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_llm_provider_definition(provider_definition_with_auth(
                "claude-sub",
                "claude-subscription",
            ))
            .unwrap();

        let err = bridge
            .sync_all("p1", &manager)
            .await
            .expect_err("dangling auth reference must fail activation");
        assert!(
            err.to_string().contains("claude-subscription"),
            "the missing auth name must stay in the message: {err}"
        );
    }
}

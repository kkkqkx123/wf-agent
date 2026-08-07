use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_types::enums::{HookType, MiddlewarePhase};

use wf_api::handler_chain::{
    PluginHandlerSource, PluginHookBridge, PluginMiddlewareBridge, PluginNodeExecutor,
};
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

    async fn unsync_all(
        &self,
        plugin_id: &str,
        _manager: &ContributionManager,
    ) -> PluginResult<()> {
        tracing::info!("[bridge] unsyncing contributions for '{}'", plugin_id);
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

    fn hook_handlers(&self, hook_type: &HookType) -> Vec<Arc<dyn PluginHookBridge>> {
        self.manager
            .get_hook_handlers(hook_type)
            .into_iter()
            .map(|handler| Arc::new(WfPluginHookBridge(handler)) as Arc<dyn PluginHookBridge>)
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

struct WfPluginHookBridge(Arc<dyn wf_plugin::PluginHookHandler>);

#[async_trait]
impl PluginHookBridge for WfPluginHookBridge {
    async fn handle(&self, _hook_type: &HookType, context: &Value) -> wf_api::ApiResult<()> {
        self.0
            .handle(context.clone())
            .await
            .map_err(wf_api::ApiError::execution_with_source)
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

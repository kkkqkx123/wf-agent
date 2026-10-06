use std::sync::Arc;

use super::runtime::Runtime;

impl Runtime {
    /// The application-facing API context assembled from the runtime's shared
    /// pieces (storage, registries, event bus, LLM gateway, tool registry).
    ///
    /// Built once and cached: the live execution handles inside the context
    /// (`WorkflowApi` / `AgentApi` pause/resume/cancel) must be shared by all
    /// callers.
    fn ensure_api_context(&self) -> &std::sync::Arc<wf_api::ApiContext> {
        self.api_ctx.get_or_init(|| {
            let storage = self.storage_manager.shared_context().expect(
                "storage not configured; set storage type to sqlite or postgres in storage.toml",
            );
            let mut ctx = wf_api::ApiContext::from_runtime_parts(
                storage,
                self.registries.clone(),
                self.event_bus.clone(),
                self.llm_gateway.clone(),
                self.tool_registry.clone(),
                self.metrics.as_ref().map(|m| m.registry().clone()),
            );
            // Plugin contributions (node-type / hook / middleware) are injected
            // into the handler resolution chain (builtin → plugin → template
            // fallback) when the plugin engine is enabled.
            #[cfg(feature = "plugins")]
            if let Some(engine) = &self.plugin_engine {
                ctx = ctx.with_plugin_source(Arc::new(
                    crate::plugin_bridge::WfPluginHandlerSource::new(
                        engine.contribution_manager().clone(),
                    ),
                ));
            }
            // Wire the durable event persistence backend; the event
            // persistence bridge restarts over the new layer.
            if let Some(persistence) = self.event_persistence.clone() {
                ctx = ctx.with_persistence(persistence);
            }
            // Wire the durable checkpoint store so execution checkpoints
            // survive restarts (crash recovery).
            ctx = ctx.with_checkpoint_store(self.checkpoint_store.clone());
            // Share the composite callback's agent loop registry so the
            // server execution views observe tool-dispatched executions.
            ctx = ctx.with_agent_loop_registry(self.agent_registry.clone());
            // Share the trigger runtime state registry so API-created
            // checkpoints capture the trigger audit trail.
            ctx = ctx.with_trigger_state_registry(self.trigger_state_registry.clone());
            // Share the hook handler registry so API-executed agents and
            // workflows fire through the same signal points.
            ctx = ctx.with_hook_handler_registry(self.hook_handler_registry.clone());
            // Attach the file checkpoint manager (file snapshots + script
            // change capture) when file checkpointing is enabled.
            if let Some(manager) = &self.file_checkpoint_manager {
                ctx = ctx.with_file_checkpoint_manager(manager.clone());
            }
            // Apply the host default tool approval config: when enabled,
            // executions launched through this context route every tool call
            // through the persisted interaction flow (the library default
            // without a handler stays auto-approve).
            ctx = ctx.with_tool_approval(self.tool_approval.clone());
            // Apply the resolved limits so executions launched through this
            // context seed node/total budgets from `execution_defaults`.
            ctx = ctx.with_execution_limits(self.limits.clone());
            std::sync::Arc::new(ctx)
        })
    }

    pub fn api_context(&self) -> &wf_api::ApiContext {
        self.ensure_api_context().as_ref()
    }

    pub fn api_context_arc(&self) -> std::sync::Arc<wf_api::ApiContext> {
        std::sync::Arc::clone(self.ensure_api_context())
    }
}

use std::sync::Arc;
use std::sync::RwLock;

use serde_json::Value;

use super::registrar::ContributionRegistrar;
use super::registries::{MultiRegistry, Registry};
use super::types::*;
use crate::error::{PluginError, PluginResult};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum OverridePolicy {
    #[default]
    Forbid,
    Warn,
    Allow,
    Priority,
}

pub struct ContributionManager {
    current_plugin_id: RwLock<String>,
    override_policy: RwLock<OverridePolicy>,
    node_type_registry: Registry<String, Arc<dyn PluginNodeHandler>>,
    tool_type_registry: Registry<String, Arc<dyn PluginToolExecutor>>,
    llm_provider_registry: Registry<String, Arc<dyn PluginLLMFormatter>>,
    formatter_registry: Registry<String, Arc<dyn PluginLLMFormatter>>,
    event_handler_registry: MultiRegistry<String, Arc<dyn PluginEventHandler>>,
    hook_handler_registry: MultiRegistry<String, Arc<dyn PluginHookHandler>>,
    middleware_registry: MultiRegistry<String, (i32, Arc<dyn PluginMiddlewareHandler>)>,
}

impl Default for ContributionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ContributionManager {
    pub fn new() -> Self {
        Self {
            current_plugin_id: RwLock::new(String::new()),
            override_policy: RwLock::new(OverridePolicy::Forbid),
            node_type_registry: Registry::new(),
            tool_type_registry: Registry::new(),
            llm_provider_registry: Registry::new(),
            formatter_registry: Registry::new(),
            event_handler_registry: MultiRegistry::new(),
            hook_handler_registry: MultiRegistry::new(),
            middleware_registry: MultiRegistry::new(),
        }
    }

    pub fn set_override_policy(&self, policy: OverridePolicy) {
        *self.override_policy.write().unwrap() = policy;
    }

    pub fn start_registration(&self, plugin_id: &str) {
        *self.current_plugin_id.write().unwrap() = plugin_id.to_owned();
    }

    pub fn as_registrar(&self) -> RegistrarGuard<'_> {
        RegistrarGuard { manager: self }
    }

    pub fn unregister_all(&self, plugin_id: &str) {
        self.node_type_registry.unregister_by_plugin(plugin_id);
        self.tool_type_registry.unregister_by_plugin(plugin_id);
        self.llm_provider_registry.unregister_by_plugin(plugin_id);
        self.formatter_registry.unregister_by_plugin(plugin_id);
        self.event_handler_registry.unregister_by_plugin(plugin_id);
        self.hook_handler_registry.unregister_by_plugin(plugin_id);
        self.middleware_registry.unregister_by_plugin(plugin_id);
    }

    // --- Query methods ---

    pub fn get_node_handler(&self, type_name: &str) -> Option<Arc<dyn PluginNodeHandler>> {
        self.node_type_registry.get(type_name)
    }

    pub fn get_tool_executor(&self, type_name: &str) -> Option<Arc<dyn PluginToolExecutor>> {
        self.tool_type_registry.get(type_name)
    }

    pub fn get_llm_formatter(&self, name: &str) -> Option<Arc<dyn PluginLLMFormatter>> {
        self.llm_provider_registry.get(name)
    }

    pub fn get_formatter(&self, name: &str) -> Option<Arc<dyn PluginLLMFormatter>> {
        self.formatter_registry.get(name)
    }

    pub fn get_event_handlers(&self, event_type: &str) -> Vec<Arc<dyn PluginEventHandler>> {
        self.event_handler_registry.get(event_type)
    }

    pub fn get_hook_handlers(&self, hook_type: &str) -> Vec<Arc<dyn PluginHookHandler>> {
        self.hook_handler_registry.get(hook_type)
    }

    pub fn get_middleware(&self, phase: &str) -> Vec<(i32, Arc<dyn PluginMiddlewareHandler>)> {
        let mut handlers = self.middleware_registry.get(phase);
        handlers.sort_by_key(|(p, _)| *p);
        handlers
    }

    // --- Enumeration methods ---

    pub fn all_node_types(&self) -> Vec<(String, String)> {
        self.node_type_registry.all()
    }

    pub fn all_tool_types(&self) -> Vec<(String, String)> {
        self.tool_type_registry.all()
    }

    pub fn all_llm_providers(&self) -> Vec<(String, String)> {
        self.llm_provider_registry.all()
    }

    pub fn all_formatters(&self) -> Vec<(String, String)> {
        self.formatter_registry.all()
    }

    pub fn all_event_handlers(&self) -> Vec<(String, String)> {
        self.event_handler_registry.all()
    }

    pub fn all_hook_handlers(&self) -> Vec<(String, String)> {
        self.hook_handler_registry.all()
    }

    pub fn all_middleware_phases(&self) -> Vec<String> {
        self.middleware_registry.keys()
    }

    pub async fn run_middleware(&self, phase: &str, context: Value) -> PluginResult<()> {
        let handlers = self.get_middleware(phase);

        let mut next: NextFn = Box::new(|| Box::pin(async { Ok(()) }));

        for (_, handler) in handlers.into_iter().rev() {
            let prev = next;
            let ctx = context.clone();
            next = Box::new(move || {
                let h = handler;
                let p = prev;
                Box::pin(async move { h.handle(ctx, p).await })
            });
        }

        next().await
    }

    fn check_conflict(
        &self,
        type_name: &str,
        key: &str,
        owner_check: impl Fn() -> Option<String>,
    ) -> PluginResult<()> {
        let policy = *self.override_policy.read().unwrap();
        if let Some(owner) = owner_check() {
            let current = self.current_plugin_id.read().unwrap().clone();
            if owner != current {
                match policy {
                    OverridePolicy::Forbid => {
                        return Err(PluginError::ContributionConflict(format!(
                            "plugin '{}' cannot override {} '{}' (owned by '{}')",
                            current, type_name, key, owner
                        )));
                    }
                    OverridePolicy::Warn => {
                        tracing::warn!(
                            "plugin '{}' overriding {} '{}' (was '{}')",
                            current,
                            type_name,
                            key,
                            owner
                        );
                    }
                    OverridePolicy::Allow | OverridePolicy::Priority => {}
                }
            }
        }
        Ok(())
    }
}

pub struct RegistrarGuard<'a> {
    manager: &'a ContributionManager,
}

impl ContributionRegistrar for RegistrarGuard<'_> {
    fn register_node_type(&mut self, type_name: &str, handler: Arc<dyn PluginNodeHandler>) {
        let plugin_id = self.manager.current_plugin_id.read().unwrap().clone();
        if self
            .manager
            .check_conflict("node-type", type_name, || {
                self.manager.node_type_registry.get_owner(type_name)
            })
            .is_ok()
        {
            self.manager
                .node_type_registry
                .register(type_name.into(), plugin_id, handler);
        }
    }

    fn register_tool_type(&mut self, type_name: &str, executor: Arc<dyn PluginToolExecutor>) {
        let plugin_id = self.manager.current_plugin_id.read().unwrap().clone();
        if self
            .manager
            .check_conflict("tool-type", type_name, || {
                self.manager.tool_type_registry.get_owner(type_name)
            })
            .is_ok()
        {
            self.manager
                .tool_type_registry
                .register(type_name.into(), plugin_id, executor);
        }
    }

    fn register_llm_provider(&mut self, name: &str, formatter: Arc<dyn PluginLLMFormatter>) {
        let plugin_id = self.manager.current_plugin_id.read().unwrap().clone();
        if self
            .manager
            .check_conflict("llm-provider", name, || {
                self.manager.llm_provider_registry.get_owner(name)
            })
            .is_ok()
        {
            self.manager
                .llm_provider_registry
                .register(name.into(), plugin_id, formatter);
        }
    }

    fn register_formatter(&mut self, name: &str, formatter: Arc<dyn PluginLLMFormatter>) {
        let plugin_id = self.manager.current_plugin_id.read().unwrap().clone();
        if self
            .manager
            .check_conflict("formatter", name, || {
                self.manager.formatter_registry.get_owner(name)
            })
            .is_ok()
        {
            self.manager
                .formatter_registry
                .register(name.into(), plugin_id, formatter);
        }
    }

    fn register_event_handler(&mut self, event_type: &str, handler: Arc<dyn PluginEventHandler>) {
        let plugin_id = self.manager.current_plugin_id.read().unwrap().clone();
        self.manager
            .event_handler_registry
            .register(event_type.into(), plugin_id, handler);
    }

    fn register_hook_handler(&mut self, hook_type: &str, handler: Arc<dyn PluginHookHandler>) {
        let plugin_id = self.manager.current_plugin_id.read().unwrap().clone();
        self.manager
            .hook_handler_registry
            .register(hook_type.into(), plugin_id, handler);
    }

    fn register_middleware(
        &mut self,
        phase: &str,
        priority: i32,
        handler: Arc<dyn PluginMiddlewareHandler>,
    ) {
        let plugin_id = self.manager.current_plugin_id.read().unwrap().clone();
        self.manager
            .middleware_registry
            .register(phase.into(), plugin_id, (priority, handler));
    }
}

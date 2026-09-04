use std::sync::Arc;
use std::sync::RwLock;

use serde_json::Value;
use wf_types::agent::AgentTemplate;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{NodeTemplate, WorkflowTemplate};
use wf_types::MiddlewarePhase;
use wf_types::SystemPromptFragment;
use wf_types::Template;

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
    llm_provider_registry: Registry<String, Arc<dyn PluginLlmFormatter>>,
    formatter_registry: Registry<String, Arc<dyn PluginLlmFormatter>>,
    event_handler_registry: MultiRegistry<String, Arc<dyn PluginEventHandler>>,
    middleware_registry: MultiRegistry<String, (i32, Arc<dyn PluginMiddlewareHandler>)>,
    // Declarative resource contribution registry (owner tracking + bridge placement)
    workflow_registry: Registry<String, Arc<WorkflowTemplate>>,
    prompt_registry: Registry<String, Arc<Template>>,
    fragment_registry: Registry<String, Arc<SystemPromptFragment>>,
    agent_template_registry: Registry<String, Arc<AgentTemplate>>,
    node_template_registry: Registry<String, Arc<NodeTemplate>>,
    trigger_registry: Registry<String, Arc<TriggerTemplate>>,
    tool_description_registry: Registry<String, Arc<ToolDescriptionData>>,
    tool_registry: Registry<String, Arc<ToolDef>>,
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
            middleware_registry: MultiRegistry::new(),
            workflow_registry: Registry::new(),
            prompt_registry: Registry::new(),
            fragment_registry: Registry::new(),
            agent_template_registry: Registry::new(),
            node_template_registry: Registry::new(),
            trigger_registry: Registry::new(),
            tool_description_registry: Registry::new(),
            tool_registry: Registry::new(),
        }
    }

    pub fn set_override_policy(&self, policy: OverridePolicy) {
        *wf_common::lock::write_ok(self.override_policy.write()) = policy;
    }

    pub fn start_registration(&self, plugin_id: &str) {
        *wf_common::lock::write_ok(self.current_plugin_id.write()) = plugin_id.to_owned();
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
        self.middleware_registry.unregister_by_plugin(plugin_id);
        self.workflow_registry.unregister_by_plugin(plugin_id);
        self.prompt_registry.unregister_by_plugin(plugin_id);
        self.fragment_registry.unregister_by_plugin(plugin_id);
        self.agent_template_registry.unregister_by_plugin(plugin_id);
        self.node_template_registry.unregister_by_plugin(plugin_id);
        self.trigger_registry.unregister_by_plugin(plugin_id);
        self.tool_description_registry
            .unregister_by_plugin(plugin_id);
        self.tool_registry.unregister_by_plugin(plugin_id);
    }

    // --- Query methods ---

    pub fn get_node_handler(&self, type_name: &str) -> Option<Arc<dyn PluginNodeHandler>> {
        self.node_type_registry.get(type_name)
    }

    pub fn get_tool_executor(&self, type_name: &str) -> Option<Arc<dyn PluginToolExecutor>> {
        self.tool_type_registry.get(type_name)
    }

    pub fn get_llm_formatter(&self, name: &str) -> Option<Arc<dyn PluginLlmFormatter>> {
        self.llm_provider_registry.get(name)
    }

    pub fn get_formatter(&self, name: &str) -> Option<Arc<dyn PluginLlmFormatter>> {
        self.formatter_registry.get(name)
    }

    pub fn get_event_handlers(&self, event_type: &str) -> Vec<Arc<dyn PluginEventHandler>> {
        self.event_handler_registry.get(event_type)
    }

    pub fn get_middleware(
        &self,
        phase: &MiddlewarePhase,
    ) -> Vec<(i32, Arc<dyn PluginMiddlewareHandler>)> {
        let mut handlers = self.middleware_registry.get(phase.as_str());
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

    pub fn all_middleware_phases(&self) -> Vec<String> {
        self.middleware_registry.keys()
    }

    /// All middleware registrations as `(phase, plugin_id)` pairs.
    pub fn all_middleware(&self) -> Vec<(String, String)> {
        self.middleware_registry.all()
    }

    // --- Resource contribution queries (consumed by the contribution bridge) ---

    pub fn all_workflows(&self) -> Vec<(String, String)> {
        self.workflow_registry.all()
    }
    pub fn get_workflow(&self, id: &str) -> Option<Arc<WorkflowTemplate>> {
        self.workflow_registry.get(id)
    }

    pub fn all_prompts(&self) -> Vec<(String, String)> {
        self.prompt_registry.all()
    }
    pub fn get_prompt(&self, id: &str) -> Option<Arc<Template>> {
        self.prompt_registry.get(id)
    }

    pub fn all_fragments(&self) -> Vec<(String, String)> {
        self.fragment_registry.all()
    }
    pub fn get_fragment(&self, id: &str) -> Option<Arc<SystemPromptFragment>> {
        self.fragment_registry.get(id)
    }

    pub fn all_agent_templates(&self) -> Vec<(String, String)> {
        self.agent_template_registry.all()
    }
    pub fn get_agent_template(&self, id: &str) -> Option<Arc<AgentTemplate>> {
        self.agent_template_registry.get(id)
    }

    pub fn all_node_templates(&self) -> Vec<(String, String)> {
        self.node_template_registry.all()
    }
    pub fn get_node_template(&self, id: &str) -> Option<Arc<NodeTemplate>> {
        self.node_template_registry.get(id)
    }

    pub fn all_triggers(&self) -> Vec<(String, String)> {
        self.trigger_registry.all()
    }
    pub fn get_trigger(&self, id: &str) -> Option<Arc<TriggerTemplate>> {
        self.trigger_registry.get(id)
    }

    pub fn all_tool_descriptions(&self) -> Vec<(String, String)> {
        self.tool_description_registry.all()
    }
    pub fn get_tool_description(&self, id: &str) -> Option<Arc<ToolDescriptionData>> {
        self.tool_description_registry.get(id)
    }

    pub fn all_tools(&self) -> Vec<(String, String)> {
        self.tool_registry.all()
    }
    pub fn get_tool(&self, id: &str) -> Option<Arc<ToolDef>> {
        self.tool_registry.get(id)
    }

    /// All contribution keys owned by `plugin_id` as `(contribution_type,
    /// key)` pairs, using the kebab-case type identifiers of the
    /// `ContributionType` union. Used to fill the registry's
    /// `ContributionRecord`s after activation.
    pub fn contributions_for(&self, plugin_id: &str) -> Vec<(String, String)> {
        let mut records: Vec<(String, String)> = Vec::new();
        for (key, owner) in self.all_node_types() {
            if owner == plugin_id {
                records.push(("node-type".into(), key));
            }
        }
        for (key, owner) in self.all_tool_types() {
            if owner == plugin_id {
                records.push(("tool-type".into(), key));
            }
        }
        for (key, owner) in self.all_llm_providers() {
            if owner == plugin_id {
                records.push(("llm-provider".into(), key));
            }
        }
        for (key, owner) in self.all_formatters() {
            if owner == plugin_id {
                records.push(("formatter".into(), key));
            }
        }
        for (key, owner) in self.all_event_handlers() {
            if owner == plugin_id {
                records.push(("event-handler".into(), key));
            }
        }
        for (key, owner) in self.all_middleware() {
            if owner == plugin_id {
                records.push(("middleware".into(), key));
            }
        }
        for (key, owner) in self.all_workflows() {
            if owner == plugin_id {
                records.push(("workflow".into(), key));
            }
        }
        for (key, owner) in self.all_prompts() {
            if owner == plugin_id {
                records.push(("prompt".into(), key));
            }
        }
        for (key, owner) in self.all_fragments() {
            if owner == plugin_id {
                records.push(("fragment".into(), key));
            }
        }
        for (key, owner) in self.all_agent_templates() {
            if owner == plugin_id {
                records.push(("agent-template".into(), key));
            }
        }
        for (key, owner) in self.all_node_templates() {
            if owner == plugin_id {
                records.push(("node-template".into(), key));
            }
        }
        for (key, owner) in self.all_triggers() {
            if owner == plugin_id {
                records.push(("trigger".into(), key));
            }
        }
        for (key, owner) in self.all_tool_descriptions() {
            if owner == plugin_id {
                records.push(("tool-description".into(), key));
            }
        }
        for (key, owner) in self.all_tools() {
            if owner == plugin_id {
                records.push(("tool".into(), key));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        records.retain(|(t, k)| seen.insert((t.clone(), k.clone())));
        records
    }

    pub async fn run_middleware(
        &self,
        phase: &MiddlewarePhase,
        context: Value,
    ) -> PluginResult<()> {
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
        let policy = *wf_common::lock::read_ok(self.override_policy.read());
        if let Some(owner) = owner_check() {
            let current = wf_common::lock::read_ok(self.current_plugin_id.read()).clone();
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
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "node-type", type_name)
            && self
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
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "tool-type", type_name)
            && self
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

    fn register_llm_provider(&mut self, name: &str, formatter: Arc<dyn PluginLlmFormatter>) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "llm-provider", name)
            && self
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

    fn register_formatter(&mut self, name: &str, formatter: Arc<dyn PluginLlmFormatter>) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "formatter", name)
            && self
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
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "event-handler", event_type) {
            self.manager
                .event_handler_registry
                .register(event_type.into(), plugin_id, handler);
        }
    }

    fn register_middleware(
        &mut self,
        phase: MiddlewarePhase,
        priority: i32,
        handler: Arc<dyn PluginMiddlewareHandler>,
    ) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        let key = phase.as_str().to_string();
        if self.validate(&plugin_id, "middleware", &key) {
            self.manager
                .middleware_registry
                .register(key, plugin_id, (priority, handler));
        }
    }

    fn register_workflow(&mut self, id: &str, wf: WorkflowTemplate) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "workflow", id)
            && self
                .manager
                .check_conflict("workflow", id, || {
                    self.manager.workflow_registry.get_owner(id)
                })
                .is_ok()
        {
            self.manager
                .workflow_registry
                .register(id.into(), plugin_id, Arc::new(wf));
        }
    }

    fn register_prompt(&mut self, id: &str, template: Template) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "prompt", id)
            && self
                .manager
                .check_conflict("prompt", id, || self.manager.prompt_registry.get_owner(id))
                .is_ok()
        {
            self.manager
                .prompt_registry
                .register(id.into(), plugin_id, Arc::new(template));
        }
    }

    fn register_fragment(&mut self, id: &str, fragment: SystemPromptFragment) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "fragment", id)
            && self
                .manager
                .check_conflict("fragment", id, || {
                    self.manager.fragment_registry.get_owner(id)
                })
                .is_ok()
        {
            self.manager
                .fragment_registry
                .register(id.into(), plugin_id, Arc::new(fragment));
        }
    }

    fn register_agent_template(&mut self, id: &str, agent: AgentTemplate) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "agent-template", id)
            && self
                .manager
                .check_conflict("agent-template", id, || {
                    self.manager.agent_template_registry.get_owner(id)
                })
                .is_ok()
        {
            self.manager
                .agent_template_registry
                .register(id.into(), plugin_id, Arc::new(agent));
        }
    }

    fn register_node_template(&mut self, id: &str, node: NodeTemplate) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "node-template", id)
            && self
                .manager
                .check_conflict("node-template", id, || {
                    self.manager.node_template_registry.get_owner(id)
                })
                .is_ok()
        {
            self.manager
                .node_template_registry
                .register(id.into(), plugin_id, Arc::new(node));
        }
    }

    fn register_trigger(&mut self, id: &str, trigger: TriggerTemplate) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "trigger", id)
            && self
                .manager
                .check_conflict("trigger", id, || {
                    self.manager.trigger_registry.get_owner(id)
                })
                .is_ok()
        {
            self.manager
                .trigger_registry
                .register(id.into(), plugin_id, Arc::new(trigger));
        }
    }

    fn register_tool_description(&mut self, id: &str, description: ToolDescriptionData) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "tool-description", id)
            && self
                .manager
                .check_conflict("tool-description", id, || {
                    self.manager.tool_description_registry.get_owner(id)
                })
                .is_ok()
        {
            self.manager.tool_description_registry.register(
                id.into(),
                plugin_id,
                Arc::new(description),
            );
        }
    }

    fn register_tool(&mut self, id: &str, tool: ToolDef) {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if self.validate(&plugin_id, "tool", id)
            && self
                .manager
                .check_conflict("tool", id, || self.manager.tool_registry.get_owner(id))
                .is_ok()
        {
            self.manager
                .tool_registry
                .register(id.into(), plugin_id, Arc::new(tool));
        }
    }
}

impl RegistrarGuard<'_> {
    /// Validate a contribution before registration (type + key). Invalid
    /// contributions are rejected with a warning and are not registered
    /// (the registrar interface is infallible: an error message is returned
    /// without registering).
    fn validate(&self, plugin_id: &str, contribution_type: &str, key: &str) -> bool {
        match crate::contributions::validation::validate_contribution(
            plugin_id,
            contribution_type,
            key,
        ) {
            Some(message) => {
                tracing::warn!(plugin_id, contribution_type, key, "{message}");
                false
            }
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributions::types::NextFn;

    struct NoopMiddlewareHandler;

    #[async_trait::async_trait]
    impl PluginMiddlewareHandler for NoopMiddlewareHandler {
        async fn handle(&self, _context: Value, _next: NextFn) -> PluginResult<()> {
            Ok(())
        }
    }

    #[test]
    fn middleware_phase_round_trips_through_registry() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        let mut registrar = manager.as_registrar();
        registrar.register_middleware(
            MiddlewarePhase::OnCheckpoint,
            10,
            Arc::new(NoopMiddlewareHandler),
        );
        assert_eq!(
            manager.get_middleware(&MiddlewarePhase::OnCheckpoint).len(),
            1
        );
        assert_eq!(
            manager
                .get_middleware(&MiddlewarePhase::from("on-checkpoint"))
                .len(),
            1
        );
        assert!(manager
            .get_middleware(&MiddlewarePhase::Other("custom-phase".into()))
            .is_empty());
    }

    #[test]
    fn resource_contributions_round_trip_by_owner() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager.as_registrar().register_prompt(
            "sys.plugin",
            Template {
                id: "sys.plugin".into(),
                name: "plugin prompt".into(),
                description: None,
                category: "system".into(),
                content: "hello from plugin".into(),
                variables: None,
                fragments: None,
            },
        );
        manager.as_registrar().register_fragment(
            "fragments.plugin.rule",
            SystemPromptFragment {
                id: "fragments.plugin.rule".into(),
                category: "constraint".into(),
                content: "plugin constraint".into(),
                description: None,
                variables: None,
            },
        );

        // Registered under the owning plugin, queryable by type.
        assert_eq!(
            manager.all_prompts(),
            vec![("sys.plugin".to_string(), "p1".to_string())]
        );
        assert!(manager.get_prompt("sys.plugin").is_some());
        assert!(manager.get_fragment("fragments.plugin.rule").is_some());
        assert!(manager
            .contributions_for("p1")
            .contains(&("prompt".to_string(), "sys.plugin".to_string())));
        assert!(manager
            .contributions_for("p1")
            .contains(&("fragment".to_string(), "fragments.plugin.rule".to_string())));

        // Override policy `Forbid` keeps the original owner: p2 cannot claim
        // the same id, so ownership stays with p1.
        manager.start_registration("p2");
        manager.as_registrar().register_prompt(
            "sys.plugin",
            Template {
                id: "sys.plugin".into(),
                name: "attempted override".into(),
                description: None,
                category: "system".into(),
                content: "nope".into(),
                variables: None,
                fragments: None,
            },
        );
        assert_eq!(
            manager.all_prompts(),
            vec![("sys.plugin".to_string(), "p1".to_string())]
        );

        // Unregistering the owner clears every resource contribution.
        manager.unregister_all("p1");
        assert!(manager.all_prompts().is_empty());
        assert!(manager.all_fragments().is_empty());
        assert!(manager
            .contributions_for("p1")
            .iter()
            .all(|(t, _)| t != "prompt" && t != "fragment"));
    }
}

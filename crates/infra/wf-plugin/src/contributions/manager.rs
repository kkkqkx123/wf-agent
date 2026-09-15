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
}

pub struct ContributionManager {
    current_plugin_id: RwLock<String>,
    override_policy: RwLock<OverridePolicy>,
    node_type_registry: Registry<String, Arc<dyn PluginNodeHandler>>,
    tool_type_registry: Registry<String, Arc<dyn PluginToolExecutor>>,
    /// Codec registry backing `LlmFormat::Custom(name)` resolution. Each
    /// entry is a low-level wire-protocol codec contributed by a plugin.
    llm_registry: Registry<String, Arc<dyn PluginLlmCodec>>,
    /// Declarative provider definitions contributed by plugins (manifest
    /// `llm_providers` segment or `register_llm_provider_definition`).
    /// The runtime bridge writes them into the gateway provider registry.
    llm_provider_definitions: Registry<String, Arc<wf_types::llm::LlmProviderDefinition>>,
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
            llm_registry: Registry::new(),
            llm_provider_definitions: Registry::new(),
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
        self.llm_registry.unregister_by_plugin(plugin_id);
        self.llm_provider_definitions
            .unregister_by_plugin(plugin_id);
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

    pub fn get_llm_codec(&self, name: &str) -> Option<Arc<dyn PluginLlmCodec>> {
        self.llm_registry.get(name)
    }

    /// Declarative provider definition contributed by a plugin.
    pub fn get_llm_provider_definition(
        &self,
        id: &str,
    ) -> Option<Arc<wf_types::llm::LlmProviderDefinition>> {
        self.llm_provider_definitions.get(id)
    }

    pub fn get_event_handlers(&self, event_type: &str) -> Vec<Arc<dyn PluginEventHandler>> {
        self.event_handler_registry.get(event_type)
    }

    pub fn get_middleware(
        &self,
        phase: &MiddlewarePhase,
    ) -> Vec<(i32, Arc<dyn PluginMiddlewareHandler>)> {
        let mut handlers = self.middleware_registry.get(phase.as_str());
        // Higher priority runs first (project-wide ordering convention).
        handlers.sort_by_key(|(p, _)| std::cmp::Reverse(*p));
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
        self.llm_registry.all()
    }

    /// All plugin-contributed provider definitions as `(id, owner)` pairs.
    pub fn all_llm_provider_definitions(&self) -> Vec<(String, String)> {
        self.llm_provider_definitions.all()
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
    /// key)` pairs, using the kebab-case identifiers of [`ContributionType`].
    /// Used to fill the registry's `ContributionRecord`s after activation.
    pub fn contributions_for(&self, plugin_id: &str) -> Vec<(String, String)> {
        let mut records: Vec<(String, String)> = Vec::new();
        for (key, owner) in self.all_node_types() {
            if owner == plugin_id {
                records.push((ContributionType::NodeType.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_tool_types() {
            if owner == plugin_id {
                records.push((ContributionType::ToolType.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_llm_providers() {
            if owner == plugin_id {
                records.push((ContributionType::LlmFormat.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_llm_provider_definitions() {
            if owner == plugin_id {
                records.push(("llm-provider-definition".to_string(), key));
            }
        }
        for (key, owner) in self.all_event_handlers() {
            if owner == plugin_id {
                records.push((ContributionType::EventHandler.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_middleware() {
            if owner == plugin_id {
                records.push((ContributionType::Middleware.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_workflows() {
            if owner == plugin_id {
                records.push((ContributionType::Workflow.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_prompts() {
            if owner == plugin_id {
                records.push((ContributionType::Prompt.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_fragments() {
            if owner == plugin_id {
                records.push((ContributionType::Fragment.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_agent_templates() {
            if owner == plugin_id {
                records.push((ContributionType::AgentTemplate.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_node_templates() {
            if owner == plugin_id {
                records.push((ContributionType::NodeTemplate.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_triggers() {
            if owner == plugin_id {
                records.push((ContributionType::Trigger.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_tool_descriptions() {
            if owner == plugin_id {
                records.push((ContributionType::ToolDescription.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_tools() {
            if owner == plugin_id {
                records.push((ContributionType::Tool.as_str().into(), key));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        records.retain(|(t, k)| seen.insert((t.clone(), k.clone())));
        records
    }

    /// Run middleware handlers for `phase` as an onion chain threaded on
    /// one context value. Higher priority runs outermost; each handler
    /// receives the context as rewritten by its outer neighbors. The
    /// returned value is the final context: handlers that skip `next`
    /// short-circuit the chain with whatever they return, and handlers
    /// may rewrite the value coming back from `next` (response rewrite).
    pub async fn run_middleware(
        &self,
        phase: &MiddlewarePhase,
        context: Value,
    ) -> PluginResult<Value> {
        let handlers = self.get_middleware(phase);

        let mut next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));

        for (_, handler) in handlers.into_iter().rev() {
            let prev = next;
            next = Box::new(move |ctx| {
                let h = handler;
                let p = prev;
                Box::pin(async move { h.handle(ctx, p).await })
            });
        }

        next(context).await
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
                    OverridePolicy::Allow => {}
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
    fn register_node_type(
        &mut self,
        type_name: &str,
        handler: Arc<dyn PluginNodeHandler>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::NodeType, type_name)?;
        self.manager
            .check_conflict(ContributionType::NodeType.as_str(), type_name, || {
                self.manager.node_type_registry.get_owner(type_name)
            })?;
        self.manager
            .node_type_registry
            .register(type_name.into(), plugin_id, handler);
        Ok(())
    }

    fn register_tool_type(
        &mut self,
        type_name: &str,
        executor: Arc<dyn PluginToolExecutor>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::ToolType, type_name)?;
        self.manager
            .check_conflict(ContributionType::ToolType.as_str(), type_name, || {
                self.manager.tool_type_registry.get_owner(type_name)
            })?;
        self.manager
            .tool_type_registry
            .register(type_name.into(), plugin_id, executor);
        Ok(())
    }

    fn register_llm_provider(
        &mut self,
        name: &str,
        codec: Arc<dyn PluginLlmCodec>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::LlmFormat, name)?;
        self.manager
            .check_conflict(ContributionType::LlmFormat.as_str(), name, || {
                self.manager.llm_registry.get_owner(name)
            })?;
        self.manager
            .llm_registry
            .register(name.into(), plugin_id, codec);
        Ok(())
    }

    fn register_llm_provider_definition(
        &mut self,
        definition: wf_types::llm::LlmProviderDefinition,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if definition.id.trim().is_empty() {
            return Err(PluginError::InvalidContribution {
                plugin_id,
                message: "provider definition id must not be empty".to_string(),
            });
        }
        if let wf_types::llm::LlmFormat::Custom(name) = &definition.format {
            if name.trim().is_empty() {
                return Err(PluginError::InvalidContribution {
                    plugin_id,
                    message: format!(
                        "provider definition '{}' format must not be empty",
                        definition.id
                    ),
                });
            }
        }
        let key = definition.id.clone();
        self.manager
            .llm_provider_definitions
            .register(key, plugin_id, Arc::new(definition));
        Ok(())
    }

    fn register_event_handler(
        &mut self,
        event_type: &str,
        handler: Arc<dyn PluginEventHandler>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::EventHandler, event_type)?;
        self.manager
            .event_handler_registry
            .register(event_type.into(), plugin_id, handler);
        Ok(())
    }

    fn register_middleware(
        &mut self,
        phase: MiddlewarePhase,
        priority: i32,
        handler: Arc<dyn PluginMiddlewareHandler>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        let key = phase.as_str().to_string();
        self.validate(&plugin_id, ContributionType::Middleware, &key)?;
        self.manager
            .middleware_registry
            .register(key, plugin_id, (priority, handler));
        Ok(())
    }

    fn register_workflow(&mut self, id: &str, wf: WorkflowTemplate) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::Workflow, id)?;
        self.manager
            .check_conflict(ContributionType::Workflow.as_str(), id, || {
                self.manager.workflow_registry.get_owner(id)
            })?;
        self.manager
            .workflow_registry
            .register(id.into(), plugin_id, Arc::new(wf));
        Ok(())
    }

    fn register_prompt(&mut self, id: &str, template: Template) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::Prompt, id)?;
        self.manager
            .check_conflict(ContributionType::Prompt.as_str(), id, || {
                self.manager.prompt_registry.get_owner(id)
            })?;
        self.manager
            .prompt_registry
            .register(id.into(), plugin_id, Arc::new(template));
        Ok(())
    }

    fn register_fragment(&mut self, id: &str, fragment: SystemPromptFragment) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::Fragment, id)?;
        self.manager
            .check_conflict(ContributionType::Fragment.as_str(), id, || {
                self.manager.fragment_registry.get_owner(id)
            })?;
        self.manager
            .fragment_registry
            .register(id.into(), plugin_id, Arc::new(fragment));
        Ok(())
    }

    fn register_agent_template(&mut self, id: &str, agent: AgentTemplate) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::AgentTemplate, id)?;
        self.manager
            .check_conflict(ContributionType::AgentTemplate.as_str(), id, || {
                self.manager.agent_template_registry.get_owner(id)
            })?;
        self.manager
            .agent_template_registry
            .register(id.into(), plugin_id, Arc::new(agent));
        Ok(())
    }

    fn register_node_template(&mut self, id: &str, node: NodeTemplate) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::NodeTemplate, id)?;
        self.manager
            .check_conflict(ContributionType::NodeTemplate.as_str(), id, || {
                self.manager.node_template_registry.get_owner(id)
            })?;
        self.manager
            .node_template_registry
            .register(id.into(), plugin_id, Arc::new(node));
        Ok(())
    }

    fn register_trigger(&mut self, id: &str, trigger: TriggerTemplate) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::Trigger, id)?;
        self.manager
            .check_conflict(ContributionType::Trigger.as_str(), id, || {
                self.manager.trigger_registry.get_owner(id)
            })?;
        self.manager
            .trigger_registry
            .register(id.into(), plugin_id, Arc::new(trigger));
        Ok(())
    }

    fn register_tool_description(
        &mut self,
        id: &str,
        description: ToolDescriptionData,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::ToolDescription, id)?;
        self.manager
            .check_conflict(ContributionType::ToolDescription.as_str(), id, || {
                self.manager.tool_description_registry.get_owner(id)
            })?;
        self.manager.tool_description_registry.register(
            id.into(),
            plugin_id,
            Arc::new(description),
        );
        Ok(())
    }

    fn register_tool(&mut self, id: &str, tool: ToolDef) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::Tool, id)?;
        self.manager
            .check_conflict(ContributionType::Tool.as_str(), id, || {
                self.manager.tool_registry.get_owner(id)
            })?;
        self.manager
            .tool_registry
            .register(id.into(), plugin_id, Arc::new(tool));
        Ok(())
    }
}

impl RegistrarGuard<'_> {
    /// Validate a contribution before registration (non-empty key). Invalid
    /// contributions are rejected with an error the caller must propagate,
    /// so a plugin never silently runs with missing contributions.
    fn validate(
        &self,
        plugin_id: &str,
        contribution_type: ContributionType,
        key: &str,
    ) -> PluginResult<()> {
        match crate::contributions::validation::validate_contribution(
            plugin_id,
            contribution_type,
            key,
        ) {
            Some(message) => {
                tracing::warn!(plugin_id, %contribution_type, key, "{message}");
                Err(PluginError::InvalidContribution {
                    plugin_id: plugin_id.to_owned(),
                    message,
                })
            }
            None => Ok(()),
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
        async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
            next(context).await
        }
    }

    /// Records the context it receives, then passes a rewrite downstream.
    struct RewriteMiddlewareHandler {
        seen: Arc<std::sync::Mutex<Vec<Value>>>,
        replacement: Value,
        call_next: bool,
    }

    #[async_trait::async_trait]
    impl PluginMiddlewareHandler for RewriteMiddlewareHandler {
        async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
            self.seen.lock().expect("seen lock poisoned").push(context);
            if self.call_next {
                next(self.replacement.clone()).await
            } else {
                Ok(self.replacement.clone())
            }
        }
    }

    fn register_for_run(
        manager: &ContributionManager,
        phase: &MiddlewarePhase,
        priority: i32,
        handler: Arc<dyn PluginMiddlewareHandler>,
    ) {
        manager.start_registration("mw-test");
        manager
            .as_registrar()
            .register_middleware(phase.clone(), priority, handler)
            .expect("middleware registers");
    }

    #[test]
    fn middleware_phase_round_trips_through_registry() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        let mut registrar = manager.as_registrar();
        registrar
            .register_middleware(
                MiddlewarePhase::OnCheckpoint,
                10,
                Arc::new(NoopMiddlewareHandler),
            )
            .unwrap();
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

    #[tokio::test]
    async fn middleware_rewrite_threads_through_chain() {
        use serde_json::json;

        let manager = ContributionManager::new();
        let phase = MiddlewarePhase::from("rewrite-phase");
        let outer_seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let inner_seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        register_for_run(
            &manager,
            &phase,
            10,
            Arc::new(RewriteMiddlewareHandler {
                seen: outer_seen.clone(),
                replacement: json!({"stage": "outer"}),
                call_next: true,
            }),
        );
        register_for_run(
            &manager,
            &phase,
            5,
            Arc::new(RewriteMiddlewareHandler {
                seen: inner_seen.clone(),
                replacement: json!({"stage": "inner"}),
                call_next: true,
            }),
        );

        let out = manager
            .run_middleware(&phase, json!({"stage": "start"}))
            .await
            .expect("chain runs");
        assert_eq!(out, json!({"stage": "inner"}));
        assert_eq!(
            *outer_seen.lock().expect("lock"),
            vec![json!({"stage": "start"})]
        );
        assert_eq!(
            *inner_seen.lock().expect("lock"),
            vec![json!({"stage": "outer"})]
        );
    }

    #[tokio::test]
    async fn middleware_short_circuit_skips_downstream() {
        use serde_json::json;

        let manager = ContributionManager::new();
        let phase = MiddlewarePhase::from("short-phase");
        let inner_seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        register_for_run(
            &manager,
            &phase,
            10,
            Arc::new(RewriteMiddlewareHandler {
                seen: Arc::new(std::sync::Mutex::new(Vec::new())),
                replacement: json!({"stopped": true}),
                call_next: false,
            }),
        );
        register_for_run(
            &manager,
            &phase,
            5,
            Arc::new(RewriteMiddlewareHandler {
                seen: inner_seen.clone(),
                replacement: json!({"never": true}),
                call_next: true,
            }),
        );

        let out = manager
            .run_middleware(&phase, json!({"stage": "start"}))
            .await
            .expect("chain runs");
        assert_eq!(out, json!({"stopped": true}));
        assert!(inner_seen.lock().expect("lock").is_empty());
    }

    #[test]
    fn resource_contributions_round_trip_by_owner() {
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_prompt(
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
            )
            .unwrap();
        manager
            .as_registrar()
            .register_fragment(
                "fragments.plugin.rule",
                SystemPromptFragment {
                    id: "fragments.plugin.rule".into(),
                    category: "constraint".into(),
                    content: "plugin constraint".into(),
                    description: None,
                    variables: None,
                },
            )
            .unwrap();

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

        // Override policy `Forbid` keeps the original owner and reports the
        // conflict to the caller: p2 cannot claim the same id, so ownership
        // stays with p1 and the registration returns an error.
        manager.start_registration("p2");
        let err = manager
            .as_registrar()
            .register_prompt(
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
            )
            .unwrap_err();
        assert!(matches!(err, PluginError::ContributionConflict(_)));
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

    #[test]
    fn allow_policy_overrides_previous_owner() {
        let manager = ContributionManager::new();
        manager.set_override_policy(OverridePolicy::Allow);
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_tool(
                "shared-tool",
                wf_types::tool::Tool {
                    id: "shared-tool".into(),
                    name: "v1".into(),
                    description: String::new(),
                    tool_type: wf_types::tool::ToolType::BuiltIn,
                    parameters: None,
                    metadata: None,
                    config: None,
                    enabled: None,
                    strict: None,
                    default_timeout_ms: None,
                },
            )
            .unwrap();
        manager.start_registration("p2");
        manager
            .as_registrar()
            .register_tool(
                "shared-tool",
                wf_types::tool::Tool {
                    id: "shared-tool".into(),
                    name: "v2".into(),
                    description: String::new(),
                    tool_type: wf_types::tool::ToolType::BuiltIn,
                    parameters: None,
                    metadata: None,
                    config: None,
                    enabled: None,
                    strict: None,
                    default_timeout_ms: None,
                },
            )
            .unwrap();
        assert_eq!(
            manager.all_tools(),
            vec![("shared-tool".to_string(), "p2".to_string())]
        );
    }

    #[test]
    fn invalid_keys_are_reported_not_dropped() {
        struct NoopTool;
        #[async_trait::async_trait]
        impl crate::contributions::types::PluginToolExecutor for NoopTool {
            async fn execute(
                &self,
                _ctx: crate::contributions::types::PluginToolContext,
            ) -> PluginResult<crate::contributions::types::PluginToolResult> {
                Ok(crate::contributions::types::PluginToolResult {
                    result: Value::Null,
                })
            }
        }
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        let err = manager
            .as_registrar()
            .register_tool_type("  ", Arc::new(NoopTool))
            .unwrap_err();
        assert!(matches!(err, PluginError::InvalidContribution { .. }));
        assert!(manager.all_tool_types().is_empty());
    }

    #[test]
    fn llm_codec_registers_custom_format() {
        struct NoopCodec;
        impl crate::contributions::types::PluginLlmCodec for NoopCodec {
            fn build_request(
                &self,
                _request: Value,
                _profile: Value,
            ) -> PluginResult<crate::contributions::types::CodecHttpRequest> {
                Err(PluginError::Internal("noop".to_string()))
            }
            fn parse_response(&self, _body: &str, _request: Value) -> PluginResult<Value> {
                Err(PluginError::Internal("noop".to_string()))
            }
        }
        let manager = ContributionManager::new();
        manager.start_registration("p1");
        manager
            .as_registrar()
            .register_llm_provider("acme", Arc::new(NoopCodec))
            .unwrap();

        assert!(manager.get_llm_codec("acme").is_some());
        assert_eq!(
            manager.all_llm_providers(),
            vec![("acme".to_string(), "p1".to_string())]
        );
        assert!(manager
            .contributions_for("p1")
            .contains(&("llm-provider".to_string(), "acme".to_string())));

        manager
            .as_registrar()
            .register_llm_provider_definition(wf_types::llm::LlmProviderDefinition {
                id: "acme".to_string(),
                name: None,
                description: None,
                base_url: Some("https://api.acme.test".to_string()),
                auth_type: Some("bearer".to_string()),
                default_headers: None,
                format: wf_types::llm::LlmFormat::Custom("ACME_CHAT".to_string()),
                model_discovery: None,
                api_version: None,
                metadata: None,
            })
            .unwrap();
        assert_eq!(
            manager.all_llm_provider_definitions(),
            vec![("acme".to_string(), "p1".to_string())]
        );

        manager.unregister_all("p1");
        assert!(manager.all_llm_providers().is_empty());
        assert!(manager.all_llm_provider_definitions().is_empty());
    }
}

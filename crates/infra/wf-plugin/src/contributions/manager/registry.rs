use super::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum OverridePolicy {
    #[default]
    Forbid,
    Warn,
    Allow,
}

pub struct ContributionManager {
    pub(crate) current_plugin_id: RwLock<String>,
    pub(crate) override_policy: RwLock<OverridePolicy>,
    pub(crate) node_type_registry: Registry<String, Arc<dyn PluginNodeHandler>>,
    pub(crate) tool_type_registry: Registry<String, Arc<dyn PluginToolExecutor>>,
    /// Codec registry backing `LlmFormat::Custom(name)` resolution. Each
    /// entry is a low-level wire-protocol codec contributed by a plugin.
    pub(crate) llm_registry: Registry<String, Arc<dyn PluginLlmCodec>>,
    /// Auth registry backing `LlmProviderDefinition::auth` references. Each
    /// entry is a provider authentication implementation contributed by a
    /// plugin (login / refresh / resolve).
    pub(crate) llm_auth_registry: Registry<String, Arc<dyn PluginLlmAuth>>,
    /// Image generation registry backing the `llm-images` contribution
    /// type. Each entry implements `generate_images` for one provider.
    /// Native backends carry network access like `llm-auth`; Lua/Wasm
    /// entries are limited to deterministic test doubles until the ABI
    /// gains a dispatch channel.
    pub(crate) llm_images_registry: Registry<String, Arc<dyn PluginLlmImages>>,
    /// Declarative provider definitions contributed by plugins (manifest
    /// `llm_providers` segment or `register_llm_provider_definition`).
    /// The runtime bridge writes them into the gateway provider registry.
    pub(crate) llm_provider_definitions:
        Registry<String, Arc<wf_types::llm::LlmProviderDefinition>>,
    pub(crate) event_handler_registry: MultiRegistry<String, Arc<dyn PluginEventHandler>>,
    pub(crate) middleware_registry: MultiRegistry<String, (i32, Arc<dyn PluginMiddlewareHandler>)>,
    // Declarative resource contribution registry (owner tracking + bridge placement)
    pub(crate) workflow_registry: Registry<String, Arc<WorkflowTemplate>>,
    pub(crate) prompt_registry: Registry<String, Arc<Template>>,
    pub(crate) fragment_registry: Registry<String, Arc<SystemPromptFragment>>,
    pub(crate) agent_template_registry: Registry<String, Arc<AgentTemplate>>,
    pub(crate) node_template_registry: Registry<String, Arc<NodeTemplate>>,
    pub(crate) trigger_registry: Registry<String, Arc<TriggerTemplate>>,
    pub(crate) tool_description_registry: Registry<String, Arc<ToolDescriptionData>>,
    pub(crate) tool_registry: Registry<String, Arc<ToolDef>>,
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
            llm_auth_registry: Registry::new(),
            llm_images_registry: Registry::new(),
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
        self.llm_auth_registry.unregister_by_plugin(plugin_id);
        self.llm_images_registry.unregister_by_plugin(plugin_id);
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

    // --- Event handler queries (core) ---
    pub fn get_event_handlers(&self, event_type: &str) -> Vec<Arc<dyn PluginEventHandler>> {
        self.event_handler_registry.get(event_type)
    }

    pub fn all_event_handlers(&self) -> Vec<(String, String)> {
        self.event_handler_registry.all()
    }

    pub(crate) fn check_conflict(
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
        for (key, owner) in self.all_llm_auths() {
            if owner == plugin_id {
                records.push((ContributionType::LlmAuth.as_str().into(), key));
            }
        }
        for (key, owner) in self.all_llm_images() {
            if owner == plugin_id {
                records.push((ContributionType::LlmImages.as_str().into(), key));
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
}

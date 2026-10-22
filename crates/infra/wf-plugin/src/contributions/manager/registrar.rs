use super::*;

pub struct RegistrarGuard<'a> {
    pub(crate) manager: &'a ContributionManager,
}


impl RegistrarGuard<'_> {
    /// Validate a contribution before registration (non-empty key). Invalid
    /// contributions are rejected with an error the caller must propagate,
    /// so a plugin never silently runs with missing contributions.
    pub(crate) fn validate(
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

impl ContributionRegistrar for RegistrarGuard<'_> {
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
}

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

    fn register_llm_auth(&mut self, name: &str, auth: Arc<dyn PluginLlmAuth>) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::LlmAuth, name)?;
        self.manager
            .check_conflict(ContributionType::LlmAuth.as_str(), name, || {
                self.manager.llm_auth_registry.get_owner(name)
            })?;
        self.manager
            .llm_auth_registry
            .register(name.into(), plugin_id, auth);
        Ok(())
    }

    fn register_llm_images(
        &mut self,
        name: &str,
        images: Arc<dyn PluginLlmImages>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::LlmImages, name)?;
        self.manager
            .check_conflict(ContributionType::LlmImages.as_str(), name, || {
                self.manager.llm_images_registry.get_owner(name)
            })?;
        self.manager
            .llm_images_registry
            .register(name.into(), plugin_id, images);
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
    fn register_compaction_advisor(
        &mut self,
        name: &str,
        advisor: Arc<dyn PluginCompactionAdvisor>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::CompactionAdvisor, name)?;
        self.manager
            .check_conflict(ContributionType::CompactionAdvisor.as_str(), name, || {
                self.manager.compaction_advisor_registry.get_owner(name)
            })?;
        self.manager
            .compaction_advisor_registry
            .register(name.into(), plugin_id, advisor);
        Ok(())
    }
    fn register_text_analyzer(
        &mut self,
        name: &str,
        analyzer: Arc<dyn PluginTextAnalyzer>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::TextAnalyzer, name)?;
        self.manager
            .check_conflict(ContributionType::TextAnalyzer.as_str(), name, || {
                self.manager.text_analyzer_registry.get_owner(name)
            })?;
        self.manager
            .text_analyzer_registry
            .register(name.into(), plugin_id, analyzer);
        Ok(())
    }
    fn register_retrieval_advisor(
        &mut self,
        name: &str,
        advisor: Arc<dyn PluginRetrievalAdvisor>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::RetrievalAdvisor, name)?;
        self.manager
            .check_conflict(ContributionType::RetrievalAdvisor.as_str(), name, || {
                self.manager.retrieval_advisor_registry.get_owner(name)
            })?;
        self.manager
            .retrieval_advisor_registry
            .register(name.into(), plugin_id, advisor);
        Ok(())
    }
    fn register_vector_encoding(
        &mut self,
        name: &str,
        encoding: Arc<dyn PluginVectorEncoding>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::VectorEncoding, name)?;
        self.manager
            .check_conflict(ContributionType::VectorEncoding.as_str(), name, || {
                self.manager.vector_encoding_registry.get_owner(name)
            })?;
        self.manager
            .vector_encoding_registry
            .register(name.into(), plugin_id, encoding);
        Ok(())
    }
    fn register_snapshot_policy(&mut self, name: &str, policy: Value) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::SnapshotPolicy, name)?;
        self.manager
            .check_conflict(ContributionType::SnapshotPolicy.as_str(), name, || {
                self.manager.snapshot_policy_registry.get_owner(name)
            })?;
        self.manager
            .snapshot_policy_registry
            .register(name.into(), plugin_id, Arc::new(policy));
        Ok(())
    }
    fn register_metadata_extractor(
        &mut self,
        name: &str,
        extractor: Arc<dyn PluginMetadataExtractor>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::MetadataExtractor, name)?;
        self.manager
            .check_conflict(ContributionType::MetadataExtractor.as_str(), name, || {
                self.manager.metadata_extractor_registry.get_owner(name)
            })?;
        self.manager
            .metadata_extractor_registry
            .register(name.into(), plugin_id, extractor);
        Ok(())
    }
    fn register_chunk_splitter(
        &mut self,
        name: &str,
        splitter: Arc<dyn PluginChunkSplitter>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::ChunkSplitter, name)?;
        self.manager
            .check_conflict(ContributionType::ChunkSplitter.as_str(), name, || {
                self.manager.chunk_splitter_registry.get_owner(name)
            })?;
        self.manager
            .chunk_splitter_registry
            .register(name.into(), plugin_id, splitter);
        Ok(())
    }
    fn register_route_selector(
        &mut self,
        name: &str,
        selector: Arc<dyn PluginRouteSelector>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::RouteSelector, name)?;
        self.manager
            .check_conflict(ContributionType::RouteSelector.as_str(), name, || {
                self.manager.route_selector_registry.get_owner(name)
            })?;
        self.manager
            .route_selector_registry
            .register(name.into(), plugin_id, selector);
        Ok(())
    }
    fn register_postprocessor(
        &mut self,
        name: &str,
        postprocessor: Arc<dyn PluginPostprocessor>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::Postprocessor, name)?;
        self.manager
            .check_conflict(ContributionType::Postprocessor.as_str(), name, || {
                self.manager.postprocessor_registry.get_owner(name)
            })?;
        self.manager
            .postprocessor_registry
            .register(name.into(), plugin_id, postprocessor);
        Ok(())
    }
    fn register_eval_judge(
        &mut self,
        name: &str,
        judge: Arc<dyn PluginEvalJudge>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::EvalJudge, name)?;
        self.manager
            .check_conflict(ContributionType::EvalJudge.as_str(), name, || {
                self.manager.eval_judge_registry.get_owner(name)
            })?;
        self.manager
            .eval_judge_registry
            .register(name.into(), plugin_id, judge);
        Ok(())
    }
    fn register_hook_handler(
        &mut self,
        name: &str,
        handler: Arc<dyn PluginHookHandler>,
    ) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        self.validate(&plugin_id, ContributionType::HookHandler, name)?;
        self.manager
            .check_conflict(ContributionType::HookHandler.as_str(), name, || {
                self.manager.hook_handler_registry.get_owner(name)
            })?;
        self.manager
            .hook_handler_registry
            .register(name.into(), plugin_id, handler);
        Ok(())
    }
    fn register_cli_command(&mut self, command: PluginCliCommand) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if let Err(message) = command.validate() {
            return Err(PluginError::InvalidContribution { plugin_id, message });
        }
        let key = command.id.clone();
        self.validate(&plugin_id, ContributionType::CliCommand, &key)?;
        self.manager
            .check_conflict(ContributionType::CliCommand.as_str(), &key, || {
                self.manager.cli_command_registry.get_owner(&key)
            })?;
        self.manager
            .cli_command_registry
            .register(key, plugin_id, Arc::new(command));
        Ok(())
    }
    fn register_completion_provider(&mut self, source: PluginCompletionSource) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if let Err(message) = source.validate() {
            return Err(PluginError::InvalidContribution { plugin_id, message });
        }
        let key = source.id.clone();
        self.validate(&plugin_id, ContributionType::CompletionProvider, &key)?;
        self.manager
            .check_conflict(ContributionType::CompletionProvider.as_str(), &key, || {
                self.manager.completion_provider_registry.get_owner(&key)
            })?;
        self.manager
            .completion_provider_registry
            .register(key, plugin_id, Arc::new(source));
        Ok(())
    }
    fn register_setting_field(&mut self, field: PluginSettingField) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if let Err(message) = field.validate() {
            return Err(PluginError::InvalidContribution { plugin_id, message });
        }
        let key = field.key.clone();
        self.validate(&plugin_id, ContributionType::SettingField, &key)?;
        self.manager
            .check_conflict(ContributionType::SettingField.as_str(), &key, || {
                self.manager.setting_field_registry.get_owner(&key)
            })?;
        self.manager
            .setting_field_registry
            .register(key, plugin_id, Arc::new(field));
        Ok(())
    }
    fn register_tool_summary(&mut self, summary: PluginToolSummary) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if let Err(message) = summary.validate() {
            return Err(PluginError::InvalidContribution { plugin_id, message });
        }
        let key = summary.tool_name.clone();
        self.validate(&plugin_id, ContributionType::ToolSummary, &key)?;
        self.manager
            .check_conflict(ContributionType::ToolSummary.as_str(), &key, || {
                self.manager.tool_summary_registry.get_owner(&key)
            })?;
        self.manager
            .tool_summary_registry
            .register(key, plugin_id, Arc::new(summary));
        Ok(())
    }
    fn register_markdown_transformer(&mut self, rule: PluginMarkdownRule) -> PluginResult<()> {
        let plugin_id = wf_common::lock::read_ok(self.manager.current_plugin_id.read()).clone();
        if let Err(message) = rule.validate() {
            return Err(PluginError::InvalidContribution { plugin_id, message });
        }
        let key = rule.id.clone();
        self.validate(&plugin_id, ContributionType::MarkdownTransformer, &key)?;
        self.manager.check_conflict(
            ContributionType::MarkdownTransformer.as_str(),
            &key,
            || self.manager.markdown_transformer_registry.get_owner(&key),
        )?;
        self.manager
            .markdown_transformer_registry
            .register(key, plugin_id, Arc::new(rule));
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

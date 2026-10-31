use std::marker::PhantomData;

use wf_types::agent::{AgentConfig, AgentDefinition, AgentHookConfig, AgentMetadata};
use wf_types::tool::AvailableTools;

use crate::ApiContext;

use super::*;

pub struct AgentDefinitionBuilder<S> {
    id: String,
    name: String,
    description: Option<String>,
    version: Option<String>,
    config: Option<AgentConfig>,
    metadata: Option<AgentMetadata>,
    created_at: i64,
    _marker: PhantomData<S>,
}

impl AgentDefinitionBuilder<DefUnnamed> {
    /// Start building an agent definition with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            description: None,
            version: None,
            config: None,
            metadata: None,
            created_at: wf_common::now(),
            _marker: PhantomData,
        }
    }

    /// Assign the agent name, entering the `DefNamed` phase.
    pub fn name(mut self, name: impl Into<String>) -> AgentDefinitionBuilder<DefNamed> {
        self.name = name.into();
        AgentDefinitionBuilder {
            id: self.id,
            name: self.name,
            description: self.description,
            version: self.version,
            config: self.config,
            metadata: self.metadata,
            created_at: self.created_at,
            _marker: PhantomData,
        }
    }
}

impl<S> AgentDefinitionBuilder<S> {
    /// Set the agent description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the agent version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the agent config.
    pub fn config(mut self, config: AgentConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set agent metadata.
    pub fn metadata(mut self, metadata: AgentMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get or create the agent config so field setters can mutate it.
    fn config_slot(&mut self) -> &mut AgentConfig {
        self.config.get_or_insert_with(empty_agent_config)
    }

    /// Set the agent system prompt.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config_slot().system_prompt = Some(prompt.into());
        self
    }

    /// Cap the number of agent iterations.
    pub fn max_iterations(mut self, max_iterations: u32) -> Self {
        self.config_slot().max_iterations = Some(max_iterations);
        self
    }

    /// Set the tools the agent may call.
    pub fn available_tools(mut self, available_tools: AvailableTools) -> Self {
        self.config_slot().available_tools = Some(available_tools);
        self
    }

    /// Replace the agent hooks.
    pub fn hooks(mut self, hooks: Vec<AgentHookConfig>) -> Self {
        self.config_slot().hooks = Some(hooks);
        self
    }

    /// Append one hook to the agent config.
    pub fn add_hook(mut self, hook: AgentHookConfig) -> Self {
        let config = self.config_slot();
        config.hooks.get_or_insert_with(Vec::new).push(hook);
        self
    }
}

/// An empty agent config (all optional fields unset) used as the base for
/// the config field setters.
fn empty_agent_config() -> AgentConfig {
    AgentConfig {
        profile_id: None,
        system_prompt: None,
        system_prompt_template_id: None,
        system_prompt_template_variables: None,
        max_iterations: None,
        max_execution_time: None,
        max_retries: None,
        execution_timeout: None,
        max_pause_duration: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        initial_messages: None,
        available_tools: None,
        stream: None,
        tool_call_protocol: None,
        hooks: None,
        dynamic_context: None,
        checkpoint: None,
        violation_policy: None,
        context_schema: None,
    }
}

impl AgentDefinitionBuilder<DefNamed> {
    /// Validate (id/name assigned, tool call format closed) and build the
    /// agent definition.
    pub fn build(self) -> crate::ApiResult<AgentDefinition> {
        let now = wf_common::now();
        let definition = AgentDefinition {
            id: self.id,
            name: self.name,
            description: self.description,
            version: self.version,
            config: self.config,
            metadata: self.metadata,
            created_at: now,
            updated_at: now,
        };
        crate::infra::config::validate_agent_definition(&definition)
            .map_err(crate::ApiError::from)?;
        Ok(definition)
    }

    /// Build and register the agent as an agent template on the shared
    /// registry. Formal save: shape plus profile and tool reference closure
    /// must pass; warnings allow registration.
    pub async fn register(self, ctx: &ApiContext) -> crate::ApiResult<()> {
        let definition = self.build()?;
        crate::agent::save_agent_template(ctx, &definition).await?;
        Ok(())
    }
}

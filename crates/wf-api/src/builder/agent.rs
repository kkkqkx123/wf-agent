//! Typed agent builders.
//!
//! Uses the same `PhantomData` phase-tracking pattern as
//! [`crate::builder::WorkflowBuilder`]: construction phases live in the type
//! system, so incomplete configurations (a tool config with no tools, a hook
//! without a hook type, an unnamed definition) are unrepresentable.
//!
//! The builders produce the static artifact types consumed by the engines:
//! [`AvailableTools`](wf_types::tool::AvailableTools), [`AgentHookConfig`],
//! agent definitions and loop configs. Value builders are pure; the
//! execution builder drives a loop through
//! [`crate::agent::agent_execution`].

use std::marker::PhantomData;
use std::sync::Arc;

use wf_core::registry::MutableRegistry;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, HookConfig};
use wf_types::agent::{
    AgentConfig, AgentDefinition, AgentHookConfig, AgentHookType, AgentMetadata,
};
use wf_types::llm::tool_call_format::ToolCallFormatConfig;
use wf_types::tool::AvailableTools;

use crate::agent::agent_execution::RunAgentLoopParams;
use crate::ApiContext;

/// Marker: no model has been assigned to the agent loop config.
#[derive(Debug)]
pub struct LoopEmpty;

/// Marker: the loop is fully configured; `build()` is available.
#[derive(Debug)]
pub struct LoopConfigured;

/// Marker: no agent definition name has been assigned yet.
#[derive(Debug)]
pub struct DefUnnamed;

/// Marker: the agent definition has a name; `build()` is available.
#[derive(Debug)]
pub struct DefNamed;

/// Marker: the hook type has not been assigned yet.
#[derive(Debug)]
pub struct HookNoType;

/// Marker: the hook type is assigned; `build()` is available.
#[derive(Debug)]
pub struct HookTyped;

/// Marker: no tools have been added yet.
#[derive(Debug)]
pub struct ToolEmpty;

/// Marker: at least one tool is available; `build()` is available.
#[derive(Debug)]
pub struct ToolBuilt;

/// Consuming builder for the available-tools contract
/// ([`AvailableTools`]).
#[derive(Debug)]
pub struct AgentToolConfigBuilder<S> {
    available: Vec<String>,
    initial: Vec<String>,
    discoverable: Vec<String>,
    hidden: Vec<String>,
    enable_general_tool: Option<bool>,
    require_approval: Vec<String>,
    allowed_workflows: Vec<String>,
    _marker: PhantomData<S>,
}

impl AgentToolConfigBuilder<ToolEmpty> {
    /// Start building an available-tools configuration.
    pub fn new() -> Self {
        Self {
            available: Vec::new(),
            initial: Vec::new(),
            discoverable: Vec::new(),
            hidden: Vec::new(),
            enable_general_tool: None,
            require_approval: Vec::new(),
            allowed_workflows: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Add a tool to the available set, entering the building phase.
    pub fn add_tool(self, name: impl Into<String>) -> AgentToolConfigBuilder<ToolBuilt> {
        let tool = name.into();
        let mut available = self.available;
        available.push(tool);
        AgentToolConfigBuilder {
            available,
            initial: self.initial,
            discoverable: self.discoverable,
            hidden: self.hidden,
            enable_general_tool: self.enable_general_tool,
            require_approval: self.require_approval,
            allowed_workflows: self.allowed_workflows,
            _marker: PhantomData,
        }
    }
}

impl Default for AgentToolConfigBuilder<ToolEmpty> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> AgentToolConfigBuilder<S> {
    /// Mark the given tools as initially enabled.
    pub fn with_initial(mut self, tools: Vec<impl Into<String>>) -> Self {
        self.initial.extend(tools.into_iter().map(Into::into));
        self
    }

    /// Mark the given tools as discoverable: only metadata is injected into
    /// the prompt and calls go through the `general` tool; schema injection
    /// happens only when a tool is activated via TOOL_VISIBILITY unblock.
    pub fn discoverable(mut self, tools: Vec<impl Into<String>>) -> Self {
        self.discoverable.extend(tools.into_iter().map(Into::into));
        self
    }

    /// Escape hatch controlling `general` tool exposure (default: auto,
    /// exposed iff the discoverable list is non-empty).
    pub fn enable_general_tool(mut self, enabled: bool) -> Self {
        self.enable_general_tool = Some(enabled);
        self
    }

    /// Explicitly hide the given tools: registered but never exposed to the
    /// model (supplements runtime visibility blocking).
    pub fn hidden(mut self, tools: Vec<impl Into<String>>) -> Self {
        self.hidden.extend(tools.into_iter().map(Into::into));
        self
    }

    /// Require human approval before the given tools can run.
    pub fn require_approval(mut self, tools: Vec<impl Into<String>>) -> Self {
        self.require_approval
            .extend(tools.into_iter().map(Into::into));
        self
    }

    /// Restrict this tool set to the given workflow ids.
    pub fn allowed_workflows(mut self, workflows: Vec<impl Into<String>>) -> Self {
        self.allowed_workflows
            .extend(workflows.into_iter().map(Into::into));
        self
    }
}

impl AgentToolConfigBuilder<ToolBuilt> {
    /// Build the [`AvailableTools`] contract.
    pub fn build(self) -> AvailableTools {
        AvailableTools {
            available: self.available,
            initial: (!self.initial.is_empty()).then_some(self.initial),
            discoverable: (!self.discoverable.is_empty()).then_some(self.discoverable),
            hidden: (!self.hidden.is_empty()).then_some(self.hidden),
            enable_general_tool: self.enable_general_tool,
            require_approval: (!self.require_approval.is_empty()).then_some(self.require_approval),
            allowed_workflows: (!self.allowed_workflows.is_empty())
                .then_some(self.allowed_workflows),
        }
    }
}

/// Consuming builder for [`AgentHookConfig`] with type-level phase tracking.
#[derive(Debug)]
pub struct AgentHookBuilder<S> {
    hook_type: AgentHookType,
    condition: Option<String>,
    event_name: String,
    event_payload: Option<serde_json::Value>,
    enabled: Option<bool>,
    weight: Option<i32>,
    create_checkpoint: Option<bool>,
    checkpoint_description: Option<String>,
    _marker: PhantomData<S>,
}

impl AgentHookBuilder<HookNoType> {
    /// Start building a hook. The event type is assigned through `hook_type`.
    pub fn new(event_name: impl Into<String>) -> Self {
        Self {
            hook_type: AgentHookType::AfterLlmCall,
            condition: None,
            event_name: event_name.into(),
            event_payload: None,
            enabled: None,
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            _marker: PhantomData,
        }
    }

    /// Assign the hook type, entering the `HookTyped` phase.
    pub fn hook_type(self, hook_type: AgentHookType) -> AgentHookBuilder<HookTyped> {
        AgentHookBuilder {
            hook_type,
            condition: self.condition,
            event_name: self.event_name,
            event_payload: self.event_payload,
            enabled: self.enabled,
            weight: self.weight,
            create_checkpoint: self.create_checkpoint,
            checkpoint_description: self.checkpoint_description,
            _marker: PhantomData,
        }
    }

    /// Hook that fires before an agent iteration.
    pub fn before_iteration(event_name: impl Into<String>) -> AgentHookBuilder<HookTyped> {
        Self::new(event_name).hook_type(AgentHookType::BeforeIteration)
    }

    /// Hook that fires after an agent iteration.
    pub fn after_iteration(event_name: impl Into<String>) -> AgentHookBuilder<HookTyped> {
        Self::new(event_name).hook_type(AgentHookType::AfterIteration)
    }

    /// Hook that fires before a tool call.
    pub fn before_tool_call(event_name: impl Into<String>) -> AgentHookBuilder<HookTyped> {
        Self::new(event_name).hook_type(AgentHookType::BeforeToolCall)
    }

    /// Hook that fires after a tool call.
    pub fn after_tool_call(event_name: impl Into<String>) -> AgentHookBuilder<HookTyped> {
        Self::new(event_name).hook_type(AgentHookType::AfterToolCall)
    }

    /// Hook that fires before an LLM call.
    pub fn before_llm_call(event_name: impl Into<String>) -> AgentHookBuilder<HookTyped> {
        Self::new(event_name).hook_type(AgentHookType::BeforeLlmCall)
    }

    /// Hook that fires after an LLM call.
    pub fn after_llm_call(event_name: impl Into<String>) -> AgentHookBuilder<HookTyped> {
        Self::new(event_name).hook_type(AgentHookType::AfterLlmCall)
    }
}

impl AgentHookBuilder<HookTyped> {
    /// Build the hook config.
    pub fn build(self) -> AgentHookConfig {
        AgentHookConfig {
            hook_type: self.hook_type,
            condition: self.condition,
            event_name: self.event_name,
            event_payload: self.event_payload,
            enabled: self.enabled,
            weight: self.weight,
            create_checkpoint: self.create_checkpoint,
            checkpoint_description: self.checkpoint_description,
            receiver: None,
        }
    }
}

impl<S> AgentHookBuilder<S> {
    /// Set the hook condition expression (evaluated against the event payload).
    pub fn condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    /// Attach a payload merged into the emitted event.
    pub fn event_payload(mut self, payload: serde_json::Value) -> Self {
        self.event_payload = Some(payload);
        self
    }

    /// Enable or disable the hook (enabled by default).
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Set the hook execution weight (ordering among same-type hooks).
    pub fn weight(mut self, weight: i32) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Create a checkpoint when the hook fires.
    pub fn create_checkpoint(mut self) -> Self {
        self.create_checkpoint = Some(true);
        self
    }

    /// Describe the checkpoint created by the hook.
    pub fn checkpoint_description(mut self, description: impl Into<String>) -> Self {
        self.checkpoint_description = Some(description.into());
        self
    }
}

/// Consuming builder for [`AgentDefinition`].
#[derive(Debug)]
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
        initial_messages: None,
        available_tools: None,
        stream: None,
        tool_call_format: None,
        hooks: None,
        dynamic_context: None,
        checkpoint: None,
        violation_policy: None,
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
    /// registry.
    pub async fn register(self, ctx: &ApiContext) -> crate::ApiResult<()> {
        let definition = self.build()?;
        let template = wf_types::agent::AgentTemplate {
            id: definition.id.clone(),
            name: definition.name.clone(),
            description: definition.description.clone().unwrap_or_default(),
            definition,
            template_category: None,
            template_tags: None,
            is_public: None,
            enabled: None,
        };
        ctx.registries
            .agent_templates
            .register(template.id.clone(), Arc::new(template))
            .map_err(|e| crate::ApiError::Conflict(e.to_string()))?;
        Ok(())
    }
}

/// Consuming builder for the agent loop runtime config
/// ([`AgentLoopConfig`]).
#[derive(Debug)]
pub struct AgentLoopConfigBuilder<S> {
    agent_id: String,
    model: String,
    max_iterations: Option<u32>,
    max_execution_time: Option<u64>,
    hooks: Vec<HookConfig>,
    available_tool_names: Vec<String>,
    initial_tool_names: Vec<String>,
    discoverable_tool_names: Vec<String>,
    enable_general_tool: Option<bool>,
    hidden_tool_names: Vec<String>,
    tool_call_format: Option<wf_types::llm::tool_call_format::ToolCallFormatConfig>,
    token_limit: Option<u64>,
    token_warning_threshold: Option<u32>,
    enable_token_tracking: Option<bool>,
    _marker: PhantomData<S>,
}

impl AgentLoopConfigBuilder<LoopEmpty> {
    /// Start building a loop config for the given agent definition id.
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            model: String::new(),
            max_iterations: None,
            max_execution_time: None,
            hooks: Vec::new(),
            available_tool_names: Vec::new(),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            hidden_tool_names: Vec::new(),
            tool_call_format: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            _marker: PhantomData,
        }
    }

    /// Assign the LLM model / profile id, entering the configured phase.
    pub fn model(mut self, model: impl Into<String>) -> AgentLoopConfigBuilder<LoopConfigured> {
        self.model = model.into();
        AgentLoopConfigBuilder {
            agent_id: self.agent_id,
            model: self.model,
            max_iterations: self.max_iterations,
            max_execution_time: self.max_execution_time,
            hooks: self.hooks,
            available_tool_names: self.available_tool_names,
            initial_tool_names: self.initial_tool_names,
            discoverable_tool_names: self.discoverable_tool_names,
            enable_general_tool: self.enable_general_tool,
            hidden_tool_names: self.hidden_tool_names,
            tool_call_format: self.tool_call_format,
            token_limit: self.token_limit,
            token_warning_threshold: self.token_warning_threshold,
            enable_token_tracking: self.enable_token_tracking,
            _marker: PhantomData,
        }
    }
}

impl<S> AgentLoopConfigBuilder<S> {
    /// Cap the number of iterations the loop may run.
    pub fn max_iterations(mut self, max_iterations: u32) -> Self {
        self.max_iterations = Some(max_iterations);
        self
    }

    /// Set the wall-clock execution budget in milliseconds.
    pub fn max_execution_time(mut self, max_execution_time: u64) -> Self {
        self.max_execution_time = Some(max_execution_time);
        self
    }

    /// Allow the loop to reference the given tool by name.
    pub fn add_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.available_tool_names.push(tool_name.into());
        self
    }

    /// Mark the given tools as discoverable: metadata-only injection,
    /// invoked through the `general` tool.
    pub fn add_discoverable_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.discoverable_tool_names.push(tool_name.into());
        self
    }

    /// Escape hatch controlling `general` tool exposure (default: auto).
    pub fn enable_general_tool(mut self, enabled: bool) -> Self {
        self.enable_general_tool = Some(enabled);
        self
    }

    /// Explicitly hide the given tools: registered but never exposed to the
    /// model (supplements runtime visibility blocking).
    pub fn add_hidden_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.hidden_tool_names.push(tool_name.into());
        self
    }

    /// Set the full tool call format config. Besides the format name, the
    /// config can carry `include_description` / `description_style`, which
    /// control the discoverable tool metadata verbosity injected into the
    /// prompt (Brief keeps the compact shape, Detailed attaches parameter
    /// descriptions).
    pub fn tool_call_format(mut self, config: ToolCallFormatConfig) -> Self {
        self.tool_call_format = Some(config);
        self
    }

    /// Add a hook built through [`AgentHookBuilder`].
    pub fn add_hook(mut self, hook: AgentHookConfig) -> Self {
        self.hooks.push(HookConfig {
            hook_type: serde_json::to_string(&hook.hook_type)
                .map(|t| t.trim_matches('"').to_string())
                .unwrap_or_else(|_| format!("{:?}", hook.hook_type)),
            condition: hook.condition,
            enabled: hook.enabled.unwrap_or(true),
            parallel: None,
            continue_on_error: None,
            receiver: hook.receiver,
        });
        self
    }
}

impl AgentLoopConfigBuilder<LoopConfigured> {
    /// Build the runtime loop config consumed by [`crate::agent::agent_execution::run`].
    pub fn build(self) -> AgentLoopConfig {
        AgentLoopConfig {
            agent_id: self.agent_id,
            model: self.model,
            max_iterations: self.max_iterations,
            max_execution_time: self.max_execution_time,
            hooks: self.hooks,
            available_tool_names: self.available_tool_names,
            initial_tool_names: self.initial_tool_names,
            discoverable_tool_names: self.discoverable_tool_names,
            enable_general_tool: self.enable_general_tool,
            activated_tool_names: Vec::new(),
            hidden_tool_names: self.hidden_tool_names,
            tool_call_format: self.tool_call_format,
            token_limit: self.token_limit,
            token_warning_threshold: self.token_warning_threshold,
            enable_token_tracking: self.enable_token_tracking,
            general_description: None,
            discoverable_metadata_block: None,
        }
    }
}

/// Consuming builder that runs an agent loop against an [`ApiContext`]
/// through [`crate::agent::agent_execution`].
pub struct AgentExecutionBuilder {
    config: AgentLoopConfig,
    message: String,
    context: std::collections::HashMap<String, serde_json::Value>,
    on_completed: Option<AgentCompletedCallback>,
}

/// Callback invoked when an agent loop completes.
type AgentCompletedCallback = Arc<dyn Fn(&str) + Send + Sync>;

impl AgentExecutionBuilder {
    /// Start building an agent execution from a built loop config.
    pub fn new(config: AgentLoopConfig) -> Self {
        Self {
            config,
            message: String::new(),
            context: Default::default(),
            on_completed: None,
        }
    }

    /// Set the initial user message driving the loop.
    pub fn with_input(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Attach a callback run with the produced `agent_loop_id` on completion.
    pub fn on_completed(mut self, on_completed: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_completed = Some(Arc::new(on_completed));
        self
    }

    /// Run the loop to completion through [`crate::agent::agent_execution::run`].
    pub async fn execute(
        self,
        ctx: &Arc<ApiContext>,
    ) -> crate::ApiResult<wf_tools::callback::AgentLoopOutput> {
        let input = AgentLoopInput {
            message: self.message,
            context: self.context,
            conversation: Vec::new(),
        };
        let output = crate::agent::agent_execution::run(
            ctx,
            RunAgentLoopParams::new(self.config.clone(), input),
        )
        .await?;
        if let Some(callback) = &self.on_completed {
            callback(&output.agent_loop_id.to_string());
        }
        Ok(output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    #[test]
    fn tool_config_requires_at_least_one_tool() {
        let config = AgentToolConfigBuilder::new()
            .add_tool("web_search")
            .with_initial(vec!["web_search"])
            .require_approval(vec!["shell_exec"])
            .build();
        assert_eq!(config.available, vec!["web_search".to_string()]);
        assert_eq!(config.initial, Some(vec!["web_search".to_string()]));
        assert_eq!(
            config.require_approval,
            Some(vec!["shell_exec".to_string()])
        );
    }

    #[test]
    fn hook_builder_produces_typed_hook() {
        let hook = AgentHookBuilder::before_tool_call("tool-audit")
            .condition("${tool_name} == 'shell'")
            .create_checkpoint()
            .build();
        assert_eq!(hook.hook_type, AgentHookType::BeforeToolCall);
        assert_eq!(hook.event_name, "tool-audit");
        assert_eq!(hook.create_checkpoint, Some(true));
    }

    #[test]
    fn agent_definition_builder_validates_and_builds() {
        let definition = AgentDefinitionBuilder::new("agent-1")
            .name("Code Agent")
            .version("1.0.0")
            .build()
            .expect("named agent must build");
        assert_eq!(definition.name, "Code Agent");
        assert_eq!(definition.id, "agent-1");
    }

    #[test]
    fn agent_definition_builder_rejects_empty_name() {
        let err = AgentDefinitionBuilder::new("agent-2")
            .name("")
            .build()
            .unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
    }

    #[test]
    fn agent_definition_builder_config_setters() {
        let definition = AgentDefinitionBuilder::new("agent-cfg")
            .name("Configured Agent")
            .system_prompt("You are a code agent.")
            .max_iterations(8)
            .available_tools(AgentToolConfigBuilder::new().add_tool("web_search").build())
            .add_hook(AgentHookBuilder::after_iteration("iter-done").build())
            .build()
            .expect("named agent must build");

        let config = definition.config.expect("config built");
        assert_eq!(
            config.system_prompt.as_deref(),
            Some("You are a code agent.")
        );
        assert_eq!(config.max_iterations, Some(8));
        assert_eq!(
            config.available_tools.as_ref().unwrap().available,
            vec!["web_search".to_string()]
        );
        assert_eq!(config.hooks.as_ref().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn agent_definition_register_adds_template() {
        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ));
        AgentDefinitionBuilder::new("agent-reg")
            .name("Registered Agent")
            .register(&ctx)
            .await
            .expect("register must succeed");
        assert!(ctx.registries.agent_templates.has("agent-reg"));
    }

    #[test]
    fn loop_config_builder_requires_model() {
        let config = AgentLoopConfigBuilder::new("agent-1")
            .model("mock")
            .max_iterations(5)
            .add_tool("web_search")
            .add_hook(
                AgentHookBuilder::after_iteration("iter-done")
                    .enabled(false)
                    .build(),
            )
            .build();
        assert_eq!(config.model, "mock");
        assert_eq!(config.max_iterations, Some(5));
        assert_eq!(config.available_tool_names, vec!["web_search".to_string()]);
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].hook_type, "AFTER_ITERATION");
        assert!(!config.hooks[0].enabled);
    }

    #[test]
    fn loop_config_builder_carries_tool_call_format_config() {
        let config = AgentLoopConfigBuilder::new("agent-1")
            .model("mock")
            .tool_call_format(ToolCallFormatConfig {
                format: wf_types::llm::ToolCallFormat::Xml,
                markers: None,
                xml_tags: None,
                include_description: Some(false),
                description_style: Some("detailed".to_string()),
                include_examples: None,
                include_rules: None,
                additional_config: None,
            })
            .build();
        let format = config.tool_call_format.expect("config must flow through");
        assert_eq!(format.format, wf_types::llm::ToolCallFormat::Xml);
        assert_eq!(format.include_description, Some(false));
        assert_eq!(format.description_style.as_deref(), Some("detailed"));
    }
}

use std::marker::PhantomData;

use wf_tools::callback::{AgentLoopConfig, HookConfig};
use wf_types::agent::AgentHookConfig;
use wf_types::llm::tool_call_protocol::ToolCallProtocolConfig;

use super::*;

pub struct AgentLoopConfigBuilder<S> {
    agent_id: String,
    model: String,
    max_iterations: Option<u32>,
    max_execution_time: Option<u64>,
    hooks: Vec<HookConfig>,
    available_tool_names: Option<Vec<String>>,
    initial_tool_names: Vec<String>,
    discoverable_tool_names: Vec<String>,
    enable_general_tool: Option<bool>,
    hidden_tool_names: Vec<String>,
    tool_call_protocol: Option<wf_types::llm::tool_call_protocol::ToolCallProtocolConfig>,
    token_limit: Option<u64>,
    token_warning_threshold: Option<u32>,
    enable_token_tracking: Option<bool>,
    history_normalization: bool,
    context_schema: Option<Vec<wf_types::TemplateVariableDefinition>>,
    context_strict: Option<bool>,
    file_scope: Option<wf_types::config::file_scope::ExecutionFileScope>,
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
            available_tool_names: None,
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            hidden_tool_names: Vec::new(),
            tool_call_protocol: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            history_normalization: false,
            context_schema: None,
            context_strict: None,
            file_scope: None,
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
            tool_call_protocol: self.tool_call_protocol,
            token_limit: self.token_limit,
            token_warning_threshold: self.token_warning_threshold,
            enable_token_tracking: self.enable_token_tracking,
            history_normalization: self.history_normalization,
            context_schema: self.context_schema,
            context_strict: self.context_strict,
            file_scope: self.file_scope,
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
        self.available_tool_names
            .get_or_insert_with(Vec::new)
            .push(tool_name.into());
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

    /// Opt into per-turn history projection (default off): each request
    /// rewrites the projected history to the current turn's exposure. Only
    /// enable when stale call shapes demonstrably confuse the model; the
    /// rewrite churns the KV-cache-friendly request prefix.
    pub fn history_normalization(mut self, enabled: bool) -> Self {
        self.history_normalization = enabled;
        self
    }

    /// Declare the shape contract for `AgentLoopInput.context`. Unknown
    /// keys stay accepted; declared keys must match their shape.
    pub fn context_schema(mut self, schema: Vec<wf_types::TemplateVariableDefinition>) -> Self {
        self.context_schema = Some(schema);
        self
    }

    /// Reject unknown non-null context keys. Declaration side owns the
    /// switch; callers never set it directly.
    pub fn context_strict(mut self, strict: bool) -> Self {
        self.context_strict = Some(strict);
        self
    }

    /// Set the full tool call format config. Besides the format name, the
    /// config can carry `include_description` / `description_style`, which
    /// control the discoverable tool metadata verbosity injected into the
    /// prompt (Brief keeps the compact shape, Detailed attaches parameter
    /// descriptions).
    pub fn tool_call_protocol(mut self, config: ToolCallProtocolConfig) -> Self {
        self.tool_call_protocol = Some(config);
        self
    }

    /// Add a hook built through [`AgentHookBuilder`].
    pub fn add_hook(mut self, hook: AgentHookConfig) -> Self {
        self.hooks.push(HookConfig::from_agent_hook(&hook));
        self
    }

    /// Set the per-execution filesystem scope. Missing inherits the parent
    /// execution then the global default; explicit values override per field.
    pub fn file_scope(mut self, scope: wf_types::config::file_scope::ExecutionFileScope) -> Self {
        self.file_scope = Some(scope);
        self
    }
}

impl AgentLoopConfigBuilder<LoopConfigured> {
    /// Build the runtime loop config consumed by [`crate::agent::agent_execution::run`].
    pub fn build(self) -> AgentLoopConfig {
        AgentLoopConfig {
            file_scope: self.file_scope,
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
            tool_call_protocol: self.tool_call_protocol,
            token_limit: self.token_limit,
            token_warning_threshold: self.token_warning_threshold,
            enable_token_tracking: self.enable_token_tracking,
            general_description: None,
            discoverable_metadata_block: None,
            history_normalization: self.history_normalization,
            context_schema: self.context_schema,
            context_strict: self.context_strict,
            checkpoint_message_interval: None,
        }
    }
}

use std::marker::PhantomData;

use wf_types::agent::{
    AgentHookConfig, AgentHookType,
};


use super::*;

pub struct AgentHookBuilder<S> {
    hook_type: AgentHookType,
    condition: Option<String>,
    event_payload: Option<serde_json::Value>,
    enabled: Option<bool>,
    priority: Option<i32>,
    create_checkpoint: Option<bool>,
    checkpoint_description: Option<String>,
    handler: Option<String>,
    _marker: PhantomData<S>,
}

impl Default for AgentHookBuilder<HookNoType> {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentHookBuilder<HookNoType> {
    /// Start building a hook. The event type is assigned through `hook_type`.
    pub fn new() -> Self {
        Self {
            hook_type: AgentHookType::AfterLlmCall,
            condition: None,
            event_payload: None,
            enabled: None,
            priority: None,
            create_checkpoint: None,
            checkpoint_description: None,
            handler: None,
            _marker: PhantomData,
        }
    }

    /// Assign the hook type, entering the `HookTyped` phase.
    pub fn hook_type(self, hook_type: AgentHookType) -> AgentHookBuilder<HookTyped> {
        AgentHookBuilder {
            hook_type,
            condition: self.condition,
            event_payload: self.event_payload,
            enabled: self.enabled,
            priority: self.priority,
            create_checkpoint: self.create_checkpoint,
            checkpoint_description: self.checkpoint_description,
            handler: self.handler,
            _marker: PhantomData,
        }
    }

    /// Hook that fires before an agent iteration.
    pub fn before_iteration() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::BeforeIteration)
    }

    /// Hook that fires after an agent iteration.
    pub fn after_iteration() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::AfterIteration)
    }

    /// Hook that fires before a tool call.
    pub fn before_tool_call() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::BeforeToolCall)
    }

    /// Hook that fires after a tool call.
    pub fn after_tool_call() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::AfterToolCall)
    }

    /// Hook that fires before an LLM call.
    pub fn before_llm_call() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::BeforeLlmCall)
    }

    /// Hook that fires after an LLM call.
    pub fn after_llm_call() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::AfterLlmCall)
    }

    /// Hook that fires once per run before the first iteration.
    pub fn before_agent() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::BeforeAgent)
    }

    /// Hook that fires once per run after the loop settles.
    pub fn after_agent() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::AfterAgent)
    }

    /// Hook that fires when a user prompt enters the loop.
    pub fn before_user_prompt() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::BeforeUserPrompt)
    }

    /// Hook that fires when a child agent is created on the parent.
    pub fn subagent_start() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::SubagentStart)
    }

    /// Hook that fires when a child agent execution settles.
    pub fn subagent_stop() -> AgentHookBuilder<HookTyped> {
        Self::new().hook_type(AgentHookType::SubagentStop)
    }
}

impl AgentHookBuilder<HookTyped> {
    /// Build the hook config.
    pub fn build(self) -> AgentHookConfig {
        AgentHookConfig {
            hook_type: self.hook_type,
            condition: self.condition,
            event_payload: self.event_payload,
            enabled: self.enabled,
            priority: self.priority,
            create_checkpoint: self.create_checkpoint,
            checkpoint_description: self.checkpoint_description,
            handler: self.handler,
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

    /// Set the hook execution priority (ordering among same-type hooks).
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
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

    /// Notify a runtime-registered handler synchronously at this hook point.
    /// Independent from the asynchronous trigger path; the two have no
    /// ordering guarantee.
    pub fn handler(mut self, handler: impl Into<String>) -> Self {
        self.handler = Some(handler.into());
        self
    }
}


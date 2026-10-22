use std::marker::PhantomData;

use wf_types::tool::AvailableTools;


use super::*;

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


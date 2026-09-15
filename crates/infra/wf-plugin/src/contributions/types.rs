// Handler traits, payload types, and `NextFn` moved to `wf-plugin-sdk`;
// re-exported here so existing `wf_plugin::contributions::types` paths stay
// valid. `ContributionType` stays host-side: it describes
// registration/override semantics owned by the engine.
pub use wf_plugin_sdk::contributions::{
    parse_middleware_outcome, MiddlewareOutcome, NextFn, PluginEventData, PluginEventHandler,
    PluginExecutionContext, PluginLlmConfig, PluginLlmFormatter, PluginLlmRequest,
    PluginLlmResponse, PluginLlmUsage, PluginMessage, PluginMiddlewareHandler, PluginNodeHandler,
    PluginNodeResult, PluginToolContext, PluginToolExecutor, PluginToolResult,
};

// ============================================================
// Contribution Type
// ============================================================

/// Every contribution kind a plugin can register, identified on the wire by
/// its kebab-case string (`as_str`). This enum is the single source of truth
/// for contribution type identifiers; validation and record bookkeeping go
/// through it instead of duplicating string literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContributionType {
    NodeType,
    ToolType,
    LlmFormat,
    Formatter,
    EventHandler,
    Middleware,
    // Declarative resource contributions (payloads from wf-types, no new dependencies)
    Workflow,
    Prompt,
    Fragment,
    AgentTemplate,
    NodeTemplate,
    Trigger,
    ToolDescription,
    Tool,
}

impl ContributionType {
    /// Kebab-case identifier used in records, validation and manifests.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NodeType => "node-type",
            Self::ToolType => "tool-type",
            Self::LlmFormat => "llm-provider",
            Self::Formatter => "formatter",
            Self::EventHandler => "event-handler",
            Self::Middleware => "middleware",
            Self::Workflow => "workflow",
            Self::Prompt => "prompt",
            Self::Fragment => "fragment",
            Self::AgentTemplate => "agent-template",
            Self::NodeTemplate => "node-template",
            Self::Trigger => "trigger",
            Self::ToolDescription => "tool-description",
            Self::Tool => "tool",
        }
    }

    /// All contribution types in declaration order.
    pub fn all() -> &'static [Self] {
        &[
            Self::NodeType,
            Self::ToolType,
            Self::LlmFormat,
            Self::Formatter,
            Self::EventHandler,
            Self::Middleware,
            Self::Workflow,
            Self::Prompt,
            Self::Fragment,
            Self::AgentTemplate,
            Self::NodeTemplate,
            Self::Trigger,
            Self::ToolDescription,
            Self::Tool,
        ]
    }
}

impl std::str::FromStr for ContributionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for candidate in Self::all() {
            if candidate.as_str() == s {
                return Ok(*candidate);
            }
        }
        Err(format!("unrecognized contribution type '{s}'"))
    }
}

impl std::fmt::Display for ContributionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Role tag distinguishing the two LLM formatter registrations sharing one
/// backing registry: a `Provider` backs `LlmFormat::Custom(name)`
/// resolution, a `Formatter` is a named message formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatterRole {
    Provider,
    Formatter,
}

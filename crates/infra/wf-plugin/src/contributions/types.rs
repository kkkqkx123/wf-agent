// Handler traits, payload types, and `NextFn` moved to `wf-plugin-sdk`;
// re-exported here so existing `wf_plugin::contributions::types` paths stay
// valid. `ContributionType` and `PluginMiddlewareDef` stay host-side: they
// describe registration/override semantics owned by the engine.
pub use wf_plugin_sdk::contributions::{
    NextFn, PluginEventData, PluginEventHandler, PluginExecutionContext, PluginLlmConfig,
    PluginLlmFormatter, PluginLlmRequest, PluginLlmResponse, PluginLlmUsage, PluginMessage,
    PluginMiddlewareHandler, PluginNodeHandler, PluginNodeResult, PluginToolContext,
    PluginToolExecutor, PluginToolResult,
};

// ============================================================
// Contribution Type
// ============================================================

#[derive(Debug, Clone)]
pub enum ContributionType {
    NodeType,
    ToolType,
    LlmProvider,
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

#[derive(Debug, Clone)]
pub struct PluginMiddlewareDef {
    pub phase: String,
    pub priority: i32,
}

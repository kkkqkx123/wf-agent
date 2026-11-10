// Handler traits, payload types, and `NextFn` moved to `wf-plugin-sdk`;
// re-exported here so existing `wf_plugin::contributions::types` paths stay
// valid. `ContributionType` stays host-side: it describes
// registration/override semantics owned by the engine.
pub use wf_plugin_sdk::contributions::{
    apply_cli_input_outcome, apply_markdown_rules, parse_cli_input_outcome,
    parse_middleware_outcome, render_tool_summary, AuthDescriptor, CliInputOutcome,
    CodecHttpRequest, MiddlewareOutcome, NextFn, PluginAuthEvent, PluginAuthNext, PluginAuthOption,
    PluginAuthPrompt, PluginCliCommand, PluginCliCondition, PluginCompactionAdvisor,
    PluginCompletionItem, PluginCompletionSource, PluginEventData, PluginEventHandler,
    PluginExecutionContext, PluginLlmAuth, PluginLlmCodec, PluginLlmImages, PluginLoginChallenge,
    PluginLoginStep, PluginMarkdownRule, PluginMiddlewareHandler, PluginNodeHandler,
    PluginNodeResult, PluginRetrievalAdvisor, PluginSettingField, PluginTextAnalyzer,
    PluginToolContext, PluginToolExecutor, PluginToolResult, PluginToolSummary,
    PluginVectorEncoding,
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
    /// Provider authentication implementation (login / refresh / resolve),
    /// referenced by a provider definition's `auth` segment.
    LlmAuth,
    /// Image generation implementation (`generate_images`). Network
    /// implementations must run on the native backend, like `llm-auth`.
    LlmImages,
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
    /// Compaction planning advice (`compaction-advisor`). Pure shaping
    /// hook; the engine validates every suggestion and falls back to the
    /// built-in planner on any failure.
    CompactionAdvisor,
    /// Deferred full-text analyzer (stage 3 reservation, no execution yet).
    TextAnalyzer,
    /// Deferred retrieval shaping advice (stage 4 reservation).
    RetrievalAdvisor,
    /// Deferred sparse and multi-vector encoders (stage 5 reservation).
    VectorEncoding,
    /// Deferred snapshot expectations as declarative JSON (stage 6 reservation).
    SnapshotPolicy,
    /// One user-facing command: slash entry, palette row and help line
    /// share the definition (`cli-command`). Declarative; the host renders.
    CliCommand,
    /// Static completion source for trigger characters (`#`, `@`, `/`).
    CompletionProvider,
    /// One settings row under `plugins.<plugin>.<name>` (`setting-field`).
    SettingField,
    /// Declarative tool-call and tool-result summary templates.
    ToolSummary,
    /// One literal Markdown rewrite applied before rendering.
    MarkdownTransformer,
}

impl ContributionType {
    /// Kebab-case identifier used in records, validation and manifests.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NodeType => "node-type",
            Self::ToolType => "tool-type",
            Self::LlmFormat => "llm-provider",
            Self::LlmAuth => "llm-auth",
            Self::LlmImages => "llm-images",
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
            Self::CompactionAdvisor => "compaction-advisor",
            Self::TextAnalyzer => "text-analyzer",
            Self::RetrievalAdvisor => "retrieval-advisor",
            Self::VectorEncoding => "vector-encoding",
            Self::SnapshotPolicy => "snapshot-policy",
            Self::CliCommand => "cli-command",
            Self::CompletionProvider => "completion-provider",
            Self::SettingField => "setting-field",
            Self::ToolSummary => "tool-summary",
            Self::MarkdownTransformer => "markdown-transformer",
        }
    }

    /// All contribution types in declaration order.
    pub fn all() -> &'static [Self] {
        &[
            Self::NodeType,
            Self::ToolType,
            Self::LlmFormat,
            Self::LlmAuth,
            Self::LlmImages,
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
            Self::CompactionAdvisor,
            Self::TextAnalyzer,
            Self::RetrievalAdvisor,
            Self::VectorEncoding,
            Self::SnapshotPolicy,
            Self::CliCommand,
            Self::CompletionProvider,
            Self::SettingField,
            Self::ToolSummary,
            Self::MarkdownTransformer,
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

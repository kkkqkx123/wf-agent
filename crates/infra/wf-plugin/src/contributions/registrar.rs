use std::sync::Arc;

use serde_json::Value;
use wf_types::agent::AgentTemplate;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{NodeTemplate, WorkflowTemplate};
use wf_types::MiddlewarePhase;
use wf_types::SystemPromptFragment;
use wf_types::Template;

use super::types::*;
use crate::error::PluginResult;

/// Registration sink handed to plugins during activation. Every method
/// reports the outcome so `Forbid`-policy conflicts and invalid keys are
/// visible to the caller instead of being silently dropped: a plugin's
/// `register_contributions` should propagate the first error with `?` so
/// activation fails loudly rather than running with missing contributions.
pub trait ContributionRegistrar {
    fn register_node_type(
        &mut self,
        type_name: &str,
        handler: Arc<dyn PluginNodeHandler>,
    ) -> PluginResult<()>;
    fn register_tool_type(
        &mut self,
        type_name: &str,
        executor: Arc<dyn PluginToolExecutor>,
    ) -> PluginResult<()>;
    fn register_llm_provider(
        &mut self,
        name: &str,
        codec: Arc<dyn PluginLlmCodec>,
    ) -> PluginResult<()>;
    /// Register a provider authentication implementation; `name` matches
    /// the `auth.provider` reference of a provider definition.
    fn register_llm_auth(&mut self, name: &str, auth: Arc<dyn PluginLlmAuth>) -> PluginResult<()>;
    /// Register an image generation implementation (`llm-images`).
    fn register_llm_images(
        &mut self,
        name: &str,
        images: Arc<dyn PluginLlmImages>,
    ) -> PluginResult<()>;
    /// Register a declarative LLM connection template. The runtime bridge
    /// writes it into the provider registry on activation and removes it
    /// symmetrically on deactivation.
    fn register_llm_provider_definition(
        &mut self,
        definition: wf_types::llm::LlmProviderDefinition,
    ) -> PluginResult<()>;
    fn register_event_handler(
        &mut self,
        event_type: &str,
        handler: Arc<dyn PluginEventHandler>,
    ) -> PluginResult<()>;
    fn register_middleware(
        &mut self,
        phase: MiddlewarePhase,
        priority: i32,
        handler: Arc<dyn PluginMiddlewareHandler>,
    ) -> PluginResult<()>;
    // Declarative resource contributions (payloads from wf-types, bridged into ResourceRegistries)
    fn register_workflow(&mut self, id: &str, wf: WorkflowTemplate) -> PluginResult<()>;
    fn register_prompt(&mut self, id: &str, template: Template) -> PluginResult<()>;
    fn register_fragment(&mut self, id: &str, fragment: SystemPromptFragment) -> PluginResult<()>;
    fn register_agent_template(&mut self, id: &str, agent: AgentTemplate) -> PluginResult<()>;
    fn register_node_template(&mut self, id: &str, node: NodeTemplate) -> PluginResult<()>;
    fn register_trigger(&mut self, id: &str, trigger: TriggerTemplate) -> PluginResult<()>;
    fn register_tool_description(
        &mut self,
        id: &str,
        description: ToolDescriptionData,
    ) -> PluginResult<()>;
    /// Register an executable tool definition (`ToolRegistry`).
    fn register_tool(&mut self, id: &str, tool: ToolDef) -> PluginResult<()>;
    /// Register compaction planning advice; `name` selects the compression
    /// strategy the advice serves. The engine validates every suggestion.
    fn register_compaction_advisor(
        &mut self,
        name: &str,
        advisor: Arc<dyn PluginCompactionAdvisor>,
    ) -> PluginResult<()>;
    /// Reserve a deferred full-text analyzer; `name` selects the analyzer.
    /// Stage 3 reservation only; the host owns backend translation.
    fn register_text_analyzer(
        &mut self,
        name: &str,
        analyzer: Arc<dyn PluginTextAnalyzer>,
    ) -> PluginResult<()>;
    /// Reserve deferred retrieval shaping advice; `name` selects the
    /// grouping, fusion or rerank policy. Stage 4 reservation only.
    fn register_retrieval_advisor(
        &mut self,
        name: &str,
        advisor: Arc<dyn PluginRetrievalAdvisor>,
    ) -> PluginResult<()>;
    /// Reserve a deferred sparse or multi-vector encoder. Stage 5
    /// reservation only; the host owns storage and dimension checks.
    fn register_vector_encoding(
        &mut self,
        name: &str,
        encoding: Arc<dyn PluginVectorEncoding>,
    ) -> PluginResult<()>;
    /// Declare deferred snapshot expectations as JSON. Stage 6 reservation
    /// only; execution stays host-owned.
    fn register_snapshot_policy(&mut self, name: &str, policy: Value) -> PluginResult<()>;
    /// Register one user-facing command (slash, palette and help share the
    /// definition). `command.id` is the registry key; a second plugin
    /// claiming the id fails loudly instead of overriding it.
    fn register_cli_command(&mut self, command: PluginCliCommand) -> PluginResult<()>;
    /// Register one static completion source for trigger characters.
    /// `source.id` is the registry key.
    fn register_completion_provider(&mut self, source: PluginCompletionSource) -> PluginResult<()>;
    /// Register one settings row under `plugins.<plugin>.<name>`.
    /// `field.key` is the registry key.
    fn register_setting_field(&mut self, field: PluginSettingField) -> PluginResult<()>;
    /// Register declarative call/result summary templates for one tool.
    /// `summary.tool_name` is the registry key.
    fn register_tool_summary(&mut self, summary: PluginToolSummary) -> PluginResult<()>;
    /// Register one literal Markdown rewrite rule. `rule.id` is the key.
    fn register_markdown_transformer(&mut self, rule: PluginMarkdownRule) -> PluginResult<()>;
}

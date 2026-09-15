use std::sync::Arc;

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
        formatter: Arc<dyn PluginLlmFormatter>,
    ) -> PluginResult<()>;
    fn register_formatter(
        &mut self,
        name: &str,
        formatter: Arc<dyn PluginLlmFormatter>,
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
}

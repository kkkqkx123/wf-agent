use std::sync::Arc;

use wf_types::agent::AgentTemplate;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{NodeTemplate, WorkflowTemplate};
use wf_types::MiddlewarePhase;
use wf_types::Template;
use wf_types::SystemPromptFragment;

use super::types::*;

pub trait ContributionRegistrar {
    fn register_node_type(&mut self, type_name: &str, handler: Arc<dyn PluginNodeHandler>);
    fn register_tool_type(&mut self, type_name: &str, executor: Arc<dyn PluginToolExecutor>);
    fn register_llm_provider(&mut self, name: &str, formatter: Arc<dyn PluginLLMFormatter>);
    fn register_formatter(&mut self, name: &str, formatter: Arc<dyn PluginLLMFormatter>);
    fn register_event_handler(&mut self, event_type: &str, handler: Arc<dyn PluginEventHandler>);
    fn register_middleware(
        &mut self,
        phase: MiddlewarePhase,
        priority: i32,
        handler: Arc<dyn PluginMiddlewareHandler>,
    );
    // —— 声明式资源贡献（载荷来自 wf-types，经桥接落入 ResourceRegistries）——
    fn register_workflow(&mut self, id: &str, wf: WorkflowTemplate);
    fn register_prompt(&mut self, id: &str, template: Template);
    fn register_fragment(&mut self, id: &str, fragment: SystemPromptFragment);
    fn register_agent_template(&mut self, id: &str, agent: AgentTemplate);
    fn register_node_template(&mut self, id: &str, node: NodeTemplate);
    fn register_trigger(&mut self, id: &str, trigger: TriggerTemplate);
    fn register_tool_description(&mut self, id: &str, description: ToolDescriptionData);
    /// Register an executable tool definition (`ToolRegistry`).
    fn register_tool(&mut self, id: &str, tool: ToolDef);
}

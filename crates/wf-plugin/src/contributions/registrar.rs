use std::sync::Arc;

use super::types::*;

pub trait ContributionRegistrar {
    fn register_node_type(&mut self, type_name: &str, handler: Arc<dyn PluginNodeHandler>);
    fn register_tool_type(&mut self, type_name: &str, executor: Arc<dyn PluginToolExecutor>);
    fn register_llm_provider(&mut self, name: &str, formatter: Arc<dyn PluginLLMFormatter>);
    fn register_formatter(&mut self, name: &str, formatter: Arc<dyn PluginLLMFormatter>);
    fn register_event_handler(&mut self, event_type: &str, handler: Arc<dyn PluginEventHandler>);
    fn register_hook_handler(&mut self, hook_type: &str, handler: Arc<dyn PluginHookHandler>);
    fn register_middleware(&mut self, phase: &str, priority: i32, handler: Arc<dyn PluginMiddlewareHandler>);
}

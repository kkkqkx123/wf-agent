use wf_tools::callback::AgentLoopConfig;
use wf_types::Id;

pub const DEFAULT_AGENT: &str = "cli";
pub const DEFAULT_MODEL: &str = "default";
pub const DEFAULT_MAX_ITERATIONS: u32 = 50;

pub fn build_agent_loop_config(agent_id: Option<String>, model: Option<String>) -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: Id::from(agent_id.unwrap_or_else(|| DEFAULT_AGENT.to_string())),
        model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        max_iterations: Some(DEFAULT_MAX_ITERATIONS),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: Vec::new(),
        initial_tool_names: Vec::new(),
        discoverable_tool_names: Vec::new(),
        enable_general_tool: None,
        activated_tool_names: Vec::new(),
        hidden_tool_names: Vec::new(),
        tool_call_format: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        general_description: None,
        discoverable_metadata_block: None,
    }
}

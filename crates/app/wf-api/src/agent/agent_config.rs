use wf_tools::callback::AgentLoopConfig;
use wf_types::Id;

pub const DEFAULT_AGENT: &str = "@standard/main";
pub const DEFAULT_MODEL: &str = "default";
pub const DEFAULT_MAX_ITERATIONS: u32 = 50;

/// Build an unresolved loop intent: `max_iterations` stays `None` so the
/// template can supply it; `resolve_and_apply` fills `DEFAULT_MAX_ITERATIONS`
/// when neither side sets it.
pub fn build_agent_loop_config(agent_id: Option<String>, model: Option<String>) -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: Id::from(agent_id.unwrap_or_else(|| DEFAULT_AGENT.to_string())),
        model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        max_iterations: None,
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: Vec::new(),
        initial_tool_names: Vec::new(),
        discoverable_tool_names: Vec::new(),
        enable_general_tool: None,
        activated_tool_names: Vec::new(),
        hidden_tool_names: Vec::new(),
        tool_call_protocol: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        general_description: None,
        discoverable_metadata_block: None,
        history_normalization: false,
        checkpoint_message_interval: None,
    }
}

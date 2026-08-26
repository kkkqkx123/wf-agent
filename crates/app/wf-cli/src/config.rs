use wf_tools::callback::AgentLoopConfig;
use wf_types::Id;

/// Default agent identifier for CLI-driven turns.
pub const DEFAULT_AGENT: &str = "cli";

/// Default LLM profile identifier when none is specified.
pub const DEFAULT_MODEL: &str = "default";

/// Default maximum iterations for an agent turn.
pub const DEFAULT_MAX_ITERATIONS: u32 = 50;

/// Build a fully populated `AgentLoopConfig` from optional CLI overrides.
///
/// Centralizes the 30+ field construction previously duplicated between
/// `run.rs` and `mini.rs` so future fields only need one update site.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_uses_defaults_when_none() {
        let cfg = build_agent_loop_config(None, None);
        assert_eq!(cfg.agent_id.to_string(), DEFAULT_AGENT);
        assert_eq!(cfg.model, DEFAULT_MODEL);
        assert_eq!(cfg.max_iterations, Some(DEFAULT_MAX_ITERATIONS));
    }

    #[test]
    fn build_respects_overrides() {
        let cfg = build_agent_loop_config(Some("my-agent".into()), Some("gpt-4".into()));
        assert_eq!(cfg.agent_id.to_string(), "my-agent");
        assert_eq!(cfg.model, "gpt-4");
    }
}

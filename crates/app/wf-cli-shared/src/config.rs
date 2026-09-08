pub use wf_api::{AgentLoopConfig, DEFAULT_AGENT, DEFAULT_MAX_ITERATIONS, DEFAULT_MODEL};

pub fn build_agent_loop_config(agent_id: Option<String>, model: Option<String>) -> AgentLoopConfig {
    wf_api::build_agent_loop_config(agent_id, model)
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

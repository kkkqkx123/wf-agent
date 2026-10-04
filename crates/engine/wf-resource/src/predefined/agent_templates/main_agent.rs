//! Built-in general-purpose main agent template (`@standard/main`).
//!
//! Serves as the default agent when a caller omits `agent_id`. Registered
//! through the standard predefined agent-template path so a user-defined
//! template with the same id takes precedence (`register_item_skip`).

use crate::embedded_assets;
use wf_types::agent::{AgentConfig, AgentDefinition, AgentMetadata, AgentTemplate};
use wf_types::tool::AvailableTools;

pub const MAIN_AGENT_TEMPLATE_ID: &str = "@standard/main";

/// Version of the embedded system prompt; checkpoint restores use it to
/// tell which prompt version drove a session. Development keeps this at
/// 1.0.0.
pub const MAIN_AGENT_PROMPT_VERSION: &str = "1.0.0";

/// Builds the built-in main agent template.
pub fn main_agent_template() -> AgentTemplate {
    let t = wf_common::now();
    AgentTemplate {
        id: MAIN_AGENT_TEMPLATE_ID.into(),
        name: "Main Agent".into(),
        description: "General-purpose coding assistant with file and shell access".into(),
        definition: AgentDefinition {
            id: MAIN_AGENT_TEMPLATE_ID.into(),
            name: "Main Agent".into(),
            description: Some("General-purpose coding assistant with file and shell access".into()),
            version: Some(MAIN_AGENT_PROMPT_VERSION.into()),
            config: Some(AgentConfig {
                profile_id: None,
                system_prompt: Some(
                    embedded_assets::agent_prompt(MAIN_AGENT_TEMPLATE_ID).to_string(),
                ),
                max_iterations: Some(50),
                max_execution_time: None,
                max_retries: None,
                execution_timeout: None,
                max_pause_duration: None,
                token_limit: None,
                token_warning_threshold: None,
                enable_token_tracking: None,
                available_tools: Some(AvailableTools {
                    available: vec![
                        "read_file".into(),
                        "write_file".into(),
                        "edit_file".into(),
                        "glob_search".into(),
                        "grep_search".into(),
                        "list_files".into(),
                        "execute_command".into(),
                        "attempt_completion".into(),
                    ],
                    initial: Some(vec![
                        "read_file".into(),
                        "glob_search".into(),
                        "grep_search".into(),
                        "list_files".into(),
                    ]),
                    discoverable: Some(vec![
                        "write_file".into(),
                        "edit_file".into(),
                        "execute_command".into(),
                        "attempt_completion".into(),
                    ]),
                    enable_general_tool: None,
                    hidden: None,
                    require_approval: Some(vec!["execute_command".into()]),
                    allowed_workflows: None,
                }),
                system_prompt_template_id: None,
                system_prompt_template_variables: None,
                initial_messages: None,
                stream: Some(true),
                tool_call_protocol: Some("native".into()),
                hooks: None,
                dynamic_context: None,
                checkpoint: None,
                violation_policy: None,
            }),
            metadata: Some(AgentMetadata {
                author: Some("system".into()),
                tags: Some(vec!["builtin".into(), "main".into(), "assistant".into()]),
                category: Some("general".into()),
            }),
            created_at: t,
            updated_at: t,
        },
        template_category: Some("general".into()),
        template_tags: Some(vec!["builtin".into(), "main".into()]),
        is_public: Some(true),
        enabled: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_is_valid_and_serializable() {
        let tmpl = main_agent_template();
        assert_eq!(tmpl.id, MAIN_AGENT_TEMPLATE_ID);
        assert_eq!(tmpl.definition.id, MAIN_AGENT_TEMPLATE_ID);
        let json = serde_json::to_string(&tmpl).expect("serialize template");
        let parsed: AgentTemplate = serde_json::from_str(&json).expect("deserialize template");
        assert_eq!(parsed, tmpl);
    }

    #[test]
    fn config_fields_use_expected_defaults() {
        let tmpl = main_agent_template();
        let config = tmpl.definition.config.as_ref().expect("config present");
        // profile_id intentionally unset: resolved from the runtime default LLM profile.
        assert!(config.profile_id.is_none());
        let tools = config.available_tools.as_ref().expect("tools present");
        assert!(tools
            .initial
            .as_ref()
            .is_some_and(|init| init.iter().all(|t| tools.available.contains(t))));
        assert!(tools
            .discoverable
            .as_ref()
            .is_some_and(|d| d.iter().all(|t| tools.available.contains(t))));
        assert_eq!(config.tool_call_protocol.as_deref(), Some("native"));
    }

    #[test]
    fn prompt_version_matches_definition_version() {
        let tmpl = main_agent_template();
        assert_eq!(
            tmpl.definition.version.as_deref(),
            Some(MAIN_AGENT_PROMPT_VERSION)
        );
    }
}

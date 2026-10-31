//! Built-in pi-compatible coding agent template (`@standard/pi-coding`).
//!
//! Mirrors the pi main agent role: an expert coding assistant with file and
//! shell access where every tool is directly declared. The toolset matches
//! the main agent, but nothing stays discoverable and the `general` proxy is
//! disabled, so the model sees the full tool schemas up front instead of
//! going through metadata indirection. The system prompt follows the pi
//! skeleton (identity, tool list, rules) while naming the real wf tools.
//! Registered through the standard predefined agent-template path so a
//! user-defined template with the same id takes precedence
//! (`register_item_skip`).

use wf_types::agent::{AgentConfig, AgentDefinition, AgentMetadata, AgentTemplate};
use wf_types::dynamic_context::DynamicContextConfig;
use wf_types::tool::AvailableTools;

pub use crate::predefined::agent_prompts::PI_CODING_AGENT_PROMPT_KEY as PI_CODING_AGENT_TEMPLATE_ID;

/// Version of the embedded system prompt; checkpoint restores use it to
/// tell which prompt version drove a session. Development keeps this at
/// 1.0.0.
pub const PI_CODING_AGENT_PROMPT_VERSION: &str = "1.0.0";

pub fn pi_coding_agent_template() -> AgentTemplate {
    let t = wf_common::now();
    AgentTemplate {
        id: PI_CODING_AGENT_TEMPLATE_ID.into(),
        name: "Pi-compatible Coding Agent".into(),
        description: "Pi-style coding assistant with all tools directly declared".into(),
        definition: AgentDefinition {
            id: PI_CODING_AGENT_TEMPLATE_ID.into(),
            name: "Pi-compatible Coding Agent".into(),
            description: Some("Pi-style coding assistant with all tools directly declared".into()),
            version: Some(PI_CODING_AGENT_PROMPT_VERSION.into()),
            config: Some(AgentConfig {
                profile_id: None,
                system_prompt: None,
                system_prompt_template_id: Some(PI_CODING_AGENT_TEMPLATE_ID.into()),
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
                    initial: None,
                    discoverable: None,
                    enable_general_tool: Some(false),
                    hidden: None,
                    require_approval: Some(vec!["execute_command".into()]),
                    allowed_workflows: None,
                }),
                system_prompt_template_variables: None,
                initial_messages: None,
                stream: Some(true),
                tool_call_protocol: Some("native".into()),
                hooks: None,
                // Surface the execution working directory (plus platform)
                // in the stable header so relative tool paths have a
                // model-visible base. No volatile tail: only the stable
                // environment section is enabled.
                dynamic_context: Some(DynamicContextConfig {
                    include_current_time: None,
                    include_todo_list: None,
                    include_workspace_files: None,
                    max_file_depth: None,
                    ignore_patterns: None,
                    include_pinned_files: None,
                    include_skills: None,
                    include_workflows: None,
                    include_environment_info: Some(true),
                    custom_sections: None,
                }),
                checkpoint: None,
                violation_policy: None,
                context_schema: None,
            }),
            metadata: Some(AgentMetadata {
                author: Some("system".into()),
                tags: Some(vec![
                    "builtin".into(),
                    "pi-coding".into(),
                    "assistant".into(),
                ]),
                category: Some("general".into()),
            }),
            created_at: t,
            updated_at: t,
        },
        template_category: Some("general".into()),
        template_tags: Some(vec!["builtin".into(), "pi-coding".into()]),
        is_public: Some(true),
        enabled: Some(true),
        is_system: Some(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_is_valid_and_serializable() {
        let tmpl = pi_coding_agent_template();
        assert_eq!(tmpl.id, PI_CODING_AGENT_TEMPLATE_ID);
        assert_eq!(tmpl.definition.id, PI_CODING_AGENT_TEMPLATE_ID);
        let json = serde_json::to_string(&tmpl).expect("serialize template");
        let parsed: AgentTemplate = serde_json::from_str(&json).expect("deserialize template");
        assert_eq!(parsed, tmpl);
    }

    #[test]
    fn all_tools_are_directly_visible() {
        let tmpl = pi_coding_agent_template();
        let tools = tmpl
            .definition
            .config
            .as_ref()
            .expect("config present")
            .available_tools
            .as_ref()
            .expect("tools present");
        assert!(tools.initial.is_none());
        assert!(tools.discoverable.is_none());
        assert_eq!(tools.enable_general_tool, Some(false));
        assert_eq!(tools.available.len(), 8);
    }

    #[test]
    fn environment_section_is_enabled_for_relative_paths() {
        let tmpl = pi_coding_agent_template();
        let dyn_cfg = tmpl
            .definition
            .config
            .as_ref()
            .expect("config present")
            .dynamic_context
            .as_ref()
            .expect("dynamic context present");
        assert_eq!(dyn_cfg.include_environment_info, Some(true));
    }

    #[test]
    fn prompt_version_matches_definition_version() {
        let tmpl = pi_coding_agent_template();
        assert_eq!(
            tmpl.definition.version.as_deref(),
            Some(PI_CODING_AGENT_PROMPT_VERSION)
        );
    }
}

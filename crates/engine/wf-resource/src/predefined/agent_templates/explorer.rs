//! Built-in read-only explorer agent template (`@standard/explorer`).
//!
//! Mirrors the Codex explorer role: fast, authoritative answers to specific,
//! well-scoped codebase questions. The template carries no write or shell
//! tools; all available tools stay initially visible so no general-tool
//! indirection is needed. Registered through the standard predefined
//! agent-template path so a user-defined template with the same id takes
//! precedence (`register_item_skip`).

use wf_types::agent::{AgentConfig, AgentDefinition, AgentMetadata, AgentTemplate};
use wf_types::tool::AvailableTools;

pub const EXPLORER_AGENT_TEMPLATE_ID: &str = "@standard/explorer";

/// Version of the embedded system prompt; checkpoint restores use it to
/// tell which prompt version drove a session. Development keeps this at
/// 1.0.0.
pub const EXPLORER_AGENT_PROMPT_VERSION: &str = "1.0.0";

const EXPLORER_AGENT_SYSTEM_PROMPT: &str = "You are a fast, authoritative codebase explorer.\n\nGuidelines:\n- Answer specific, well-scoped questions about the codebase; do not expand the scope.\n- Trust prior explorer findings and reuse them instead of re-exploring the same problem.\n- You may inspect code yourself for context, but never modify files or run shell commands.\n- Prefer dedicated read tools (read_file, grep_search, glob_search, list_files) for inspection.\n- When several independent questions exist, answer the one you were given and keep the result self-contained so the caller can fan out other explorers in parallel.\n- Report findings concisely with file paths and relevant symbols; state uncertainty explicitly.\n- Finish by answering directly once no more tool calls are needed; call attempt_completion only when the caller asked for an explicit completion signal.";

pub fn explorer_agent_template() -> AgentTemplate {
    let t = wf_common::now();
    AgentTemplate {
        id: EXPLORER_AGENT_TEMPLATE_ID.into(),
        name: "Explorer".into(),
        description: "Read-only codebase explorer for specific, well-scoped questions".into(),
        definition: AgentDefinition {
            id: EXPLORER_AGENT_TEMPLATE_ID.into(),
            name: "Explorer".into(),
            description: Some(
                "Read-only codebase explorer for specific, well-scoped questions".into(),
            ),
            version: Some(EXPLORER_AGENT_PROMPT_VERSION.into()),
            config: Some(AgentConfig {
                profile_id: None,
                system_prompt: Some(EXPLORER_AGENT_SYSTEM_PROMPT.into()),
                max_iterations: Some(15),
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
                        "glob_search".into(),
                        "grep_search".into(),
                        "list_files".into(),
                        "attempt_completion".into(),
                    ],
                    initial: None,
                    discoverable: None,
                    enable_general_tool: Some(false),
                    hidden: None,
                    require_approval: None,
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
                tags: Some(vec!["builtin".into(), "explorer".into()]),
                category: Some("exploration".into()),
            }),
            created_at: t,
            updated_at: t,
        },
        template_category: Some("exploration".into()),
        template_tags: Some(vec!["builtin".into(), "explorer".into()]),
        is_public: Some(true),
        enabled: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_is_valid_and_serializable() {
        let tmpl = explorer_agent_template();
        assert_eq!(tmpl.id, EXPLORER_AGENT_TEMPLATE_ID);
        assert_eq!(tmpl.definition.id, EXPLORER_AGENT_TEMPLATE_ID);
        let json = serde_json::to_string(&tmpl).expect("serialize template");
        let parsed: AgentTemplate = serde_json::from_str(&json).expect("deserialize template");
        assert_eq!(parsed, tmpl);
    }

    #[test]
    fn toolset_is_read_only() {
        let tmpl = explorer_agent_template();
        let tools = tmpl
            .definition
            .config
            .as_ref()
            .expect("config present")
            .available_tools
            .as_ref()
            .expect("tools present");
        for name in &tools.available {
            assert!(
                matches!(
                    name.as_str(),
                    "read_file"
                        | "glob_search"
                        | "grep_search"
                        | "list_files"
                        | "attempt_completion"
                ),
                "explorer must stay read-only, got {name}"
            );
        }
        assert_eq!(tools.enable_general_tool, Some(false));
    }

    #[test]
    fn prompt_version_matches_definition_version() {
        let tmpl = explorer_agent_template();
        assert_eq!(
            tmpl.definition.version.as_deref(),
            Some(EXPLORER_AGENT_PROMPT_VERSION)
        );
    }
}

//! Sub-agent lifecycle reporting to the parent stream.
//!
//! When a parent run streams to a live consumer, the child engine reports
//! its start and finish through the typed parent link so the parent can
//! attribute the child to the dispatching tool call. Delivery is best
//! effort: a full or closed channel drops the event instead of stalling
//! the child.

use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, ParentLink};
use wf_types::events::ExecutionStreamEvent;
use wf_types::Id;

/// Maximum preview characters carried on the start event.
const PREVIEW_CHARS: usize = 160;

/// Maximum display-name characters derived from the task prompt.
const NAME_CHARS: usize = 60;

/// Display name for a dispatched child: the agent definition id when set,
/// otherwise the first line of the task prompt.
pub fn display_name(config: &AgentLoopConfig, input: &AgentLoopInput) -> String {
    if !config.agent_id.as_str().trim().is_empty() {
        return config.agent_id.to_string();
    }
    let first = input
        .message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("subagent");
    truncate_chars(first, NAME_CHARS)
}

/// Truncated task preview for the start event.
pub fn preview(input: &AgentLoopInput) -> String {
    let flat = input
        .message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate_chars(flat.trim(), PREVIEW_CHARS)
}

/// Nesting depth of the child about to run (root = 0).
pub fn child_depth(parent: &ParentLink) -> u32 {
    parent.manager.depth().saturating_add(1)
}

/// Report a child start to the parent stream. No observer means no event.
pub fn emit_started(
    parent: &ParentLink,
    child_id: &Id,
    config: &AgentLoopConfig,
    input: &AgentLoopInput,
    background: bool,
) {
    let Some(tx) = parent.subagent_tx.as_ref() else {
        return;
    };
    let _ = tx.try_send(ExecutionStreamEvent::SubAgentStarted {
        id: child_id.to_string(),
        name: display_name(config, input),
        tool_call_id: parent.tool_call_id.clone(),
        depth: Some(child_depth(parent)),
        preview: Some(preview(input)),
        background: Some(background),
    });
}

/// Report a child finish to the parent stream. No observer means no event.
pub fn emit_ended(parent: &ParentLink, child_id: &Id, name: &str, success: bool, background: bool) {
    let Some(tx) = parent.subagent_tx.as_ref() else {
        return;
    };
    let _ = tx.try_send(ExecutionStreamEvent::SubAgentEnded {
        id: child_id.to_string(),
        name: name.to_string(),
        success,
        tool_call_id: parent.tool_call_id.clone(),
        background: Some(background),
    });
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(agent_id: &str) -> AgentLoopConfig {
        AgentLoopConfig {
            file_scope: None,
            agent_id: Id::from(agent_id.to_string()),
            model: "mock".to_string(),
            max_iterations: Some(3),
            max_execution_time: None,
            hooks: Vec::new(),
            available_tool_names: Some(Vec::new()),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            activated_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
            tool_call_protocol: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: Some(false),
            general_description: None,
            discoverable_metadata_block: None,
            history_normalization: false,
            context_schema: None,
            context_strict: None,
            checkpoint_message_interval: None,
        }
    }

    fn input(message: &str) -> AgentLoopInput {
        AgentLoopInput {
            message: message.to_string(),
            context: std::collections::HashMap::new(),
            conversation: Vec::new(),
        }
    }

    #[test]
    fn display_name_prefers_agent_id() {
        assert_eq!(
            display_name(&config("researcher"), &input("do work")),
            "researcher"
        );
    }

    #[test]
    fn display_name_falls_back_to_prompt_first_line() {
        assert_eq!(
            display_name(&config(""), &input("\n  Summarize the repo\nsecond")),
            "Summarize the repo"
        );
    }

    #[test]
    fn preview_collapses_whitespace_and_truncates() {
        let long = "word ".repeat(100);
        let got = preview(&input(&long));
        assert!(got.chars().count() <= PREVIEW_CHARS);
        assert!(!got.contains('\n'));
    }
}
